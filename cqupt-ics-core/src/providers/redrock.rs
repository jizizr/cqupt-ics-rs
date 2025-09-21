use std::collections::HashMap;

use crate::{
    Course, CourseRequest, CourseResponse, Error, RecurrenceRule, Result,
    prelude::*,
    providers::{BaseProvider, Provider},
};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

const LESSON_TIMES: [(usize, usize); 11] = [
    (8 * 60, 8 * 60 + 45),        // 第1节: 08:00-08:45
    (8 * 60 + 55, 9 * 60 + 40),   // 第2节: 08:55-09:40
    (10 * 60, 10 * 60 + 45),      // 第3节: 10:00-10:45
    (10 * 60 + 55, 11 * 60 + 40), // 第4节: 10:55-11:40
    (14 * 60, 14 * 60 + 45),      // 第5节: 14:00-14:45
    (14 * 60 + 55, 15 * 60 + 40), // 第6节: 14:55-15:40
    (16 * 60, 16 * 60 + 45),      // 第7节: 16:00-16:45
    (16 * 60 + 55, 17 * 60 + 40), // 第8节: 16:55-17:40
    (19 * 60, 19 * 60 + 45),      // 第9节: 19:00-19:45
    (19 * 60 + 55, 20 * 60 + 40), // 第10节: 19:55-20:40
    (20 * 60 + 50, 21 * 60 + 35), // 第11节: 20:50-21:35
];

/// Redrock API响应数据结构
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RedrockResponse {
    data: Vec<RedrockClass>,
    info: String,
    #[serde(rename = "nowWeek")]
    now_week: u32,
    status: u32,
    #[serde(rename = "stuNum")]
    stu_num: String,
    version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedrockToken {
    pub data: RedrockTokenData,
    pub info: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedrockTokenData {
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    pub token: String,
}

/// Redrock课程信息
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RedrockClass {
    #[serde(rename = "hash_day")]
    hash_day: u32,
    #[serde(rename = "hash_lesson")]
    hash_lesson: u32,
    course: String,
    teacher: String,
    #[serde(rename = "course_num")]
    course_num: String,
    #[serde(rename = "type")]
    course_type: String,
    classroom: String,
    day: String,
    lesson: String,
    #[serde(rename = "rawWeek")]
    raw_week: String,
    #[serde(rename = "weekModel")]
    week_model: String,
    period: u32,
    week: Vec<u32>,
    #[serde(rename = "begin_lesson")]
    begin_lesson: u32,
    #[serde(rename = "week_begin")]
    week_begin: u32,
    #[serde(rename = "week_end")]
    week_end: u32,
}

/// 考试响应数据结构
#[derive(Debug, Deserialize)]
struct ExamResponse {
    data: Vec<RedrockExam>,
    #[serde(rename = "nowWeek")]
    now_week: u32,
}

/// Redrock考试信息
#[derive(Debug, Deserialize)]
struct RedrockExam {
    course: String,
    #[serde(rename = "begin_time")]
    begin_time: String,
    #[serde(rename = "end_time")]
    end_time: String,
    status: String,
    classroom: String,
    #[serde(rename = "type")]
    exam_type: String,
    week: String,
    weekday: String,
    seat: Option<String>,
}

pub struct RedrockProvider {
    base: BaseProvider,
}

impl RedrockProvider {
    const API_ROOT: &'static str = "https://be-prod.redrock.cqupt.edu.cn";
    pub fn new() -> Self {
        let mut base = BaseProviderBuilder::new(ProviderInfo {
            name: "redrock".to_string(),
            description: "Redrock API".to_string(),
        });
        base.client_builder = base
            .client_builder
            .user_agent("zhang shang zhong you/6.1.1 (iPhone; iOS 14.6; Scale/3.00)");

        Self { base: base.build() }
    }
}

impl Default for RedrockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl RedrockProvider {
    async fn authenticate(&self, credentials: &crate::types::Credentials) -> Result<RedrockToken> {
        tracing::info!("Authenticating user: {}", credentials.username);
        let url = format!("{}/magipoke/token", Self::API_ROOT);
        let mut data = HashMap::new();
        data.insert("stuNum", credentials.username.clone());
        data.insert("idNum", credentials.password.clone());
        let response = self
            .base
            .client
            .post(&url)
            .header("Host", Self::API_ROOT.trim_start_matches("https://"))
            .json(&data)
            .send()
            .await
            .map_err(|e| self.base.handle_error_req(e))?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(self
                .base
                .custom_error(format!("HTTP {} error", response.status())));
        }
        response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse response: {}", e))
        })
    }

    /// 解析 version 字段来计算学期开始时间
    /// version 格式如 "2025.9.8" 表示2025年9月8日为第一周开始
    fn parse_semester_start_from_version(&self, version: &str) -> Result<DateTime<FixedOffset>> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(self.base.custom_error(format!(
                "Invalid version format: {}, expected YYYY.M.D",
                version
            )));
        }

        let year: i32 = parts[0].parse().map_err(|_| {
            self.base
                .custom_error(format!("Invalid year in version: {}", parts[0]))
        })?;

        let month: u32 = parts[1].parse().map_err(|_| {
            self.base
                .custom_error(format!("Invalid month in version: {}", parts[1]))
        })?;

        let day: u32 = parts[2].parse().map_err(|_| {
            self.base
                .custom_error(format!("Invalid day in version: {}", parts[2]))
        })?;

        let semester_start =
            chrono::NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
                self.base.custom_error(format!(
                    "Invalid date from version: {}-{}-{}",
                    year, month, day
                ))
            })?;

        // 找到这一周的星期一
        let days_since_monday = semester_start.weekday().num_days_from_monday();
        let first_monday = semester_start - chrono::Duration::days(days_since_monday as i64);

        let naive_midnight = first_monday
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| self.base.custom_error("Failed to create datetime"))?;
        let tz = self.timezone();
        let dt_cst = tz.from_local_datetime(&naive_midnight).single().unwrap();
        Ok(dt_cst)
    }

    fn get_semester_start_from_now_week(&self, now_week: u32) -> Result<DateTime<FixedOffset>> {
        if now_week == 0 {
            return Err(self
                .base
                .custom_error("now_week is 0, cannot determine semester start".to_string()));
        }

        let tz = self.timezone();

        let now_local = Utc::now().with_timezone(&tz);
        let today_local = now_local.date_naive();

        let days_since_monday = today_local.weekday().num_days_from_monday() as i64;

        let weeks_back = (now_week - 1) as i64;

        let total_days_back = days_since_monday + weeks_back * 7;

        let start_day_local = today_local
            .checked_sub_signed(Duration::days(total_days_back))
            .ok_or_else(|| {
                self.base
                    .custom_error("date underflow when subtracting days".to_string())
            })?;

        let naive_midnight = start_day_local.and_hms_opt(0, 0, 0).ok_or_else(|| {
            self.base
                .custom_error("failed to create midnight".to_string())
        })?;

        let start_local = tz
            .from_local_datetime(&naive_midnight)
            .single()
            .ok_or_else(|| {
                self.base
                    .custom_error("invalid local datetime for +08:00".to_string())
            })?;

        Ok(start_local)
    }

    async fn get_class_schedule_data(
        &self,
        student_id: &str,
        token: &RedrockToken,
    ) -> Result<RedrockResponse> {
        let url = format!("{}/magipoke-jwzx/kebiao", Self::API_ROOT);

        let mut data = HashMap::new();
        data.insert(
            "stu_num",
            student_id
                .parse::<u32>()
                .map_err(|_| Error::Config("Invalid student ID".to_string()))?,
        );
        let response = self
            .base
            .client
            .post(&url)
            .bearer_auth(&token.data.token)
            .form(&data)
            .send()
            .await
            .map_err(|e| self.base.handle_error_req(e))?;

        if !response.status().is_success() {
            return Err(self
                .base
                .custom_error(format!("HTTP {} error", response.status())));
        }

        response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse response: {}", e))
        })
    }
    /// 获取课程表数据
    async fn get_class_schedule(
        &self,
        request: &mut CourseRequest,
        token: &RedrockToken,
    ) -> Result<(Vec<Course>, u32)> {
        let redrock_response: RedrockResponse = self
            .get_class_schedule_data(&request.credentials.username, token)
            .await?;

        // 确定学期开始时间，并获取 start_date 的引用
        let start_date: &DateTime<FixedOffset> = &request
            .semester
            .get_or_insert_with(|| {
                let start_date = if redrock_response.now_week == 0 {
                    self.parse_semester_start_from_version(&redrock_response.version)
                        .expect("failed to parse semester start")
                } else {
                    self.get_semester_start_from_now_week(redrock_response.now_week)
                        .expect("failed to get semester start")
                };
                Semester { start_date }
            })
            .start_date;
        println!(
            "学期开始日期: {}",
            start_date
                .with_timezone(&self.timezone())
                .format("%Y-%m-%d")
        );
        // 转换为 Course 结构
        let mut courses = Vec::new();
        for class in redrock_response.data {
            let course = self.convert_class_to_course_with_recurrence(&class, start_date)?;
            courses.push(course);
        }

        Ok((courses, redrock_response.now_week))
    }

    /// 获取考试安排
    async fn get_exam_schedule(&self, student_id: &str) -> Result<(Vec<Course>, u32)> {
        let url = format!("{}/magipoke-jwzx/examSchedule", Self::API_ROOT);

        let mut data = HashMap::new();
        data.insert(
            "stuNum",
            student_id
                .parse::<u32>()
                .map_err(|_| Error::Config("Invalid student ID".to_string()))?,
        );

        let response = self
            .base
            .client
            .post(&url)
            .form(&data)
            .send()
            .await
            .map_err(|e| self.base.handle_error_req(e))?;

        if !response.status().is_success() {
            return Err(self
                .base
                .custom_error(format!("HTTP {} error", response.status())));
        }

        let exam_response: ExamResponse = response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse response: {}", e))
        })?;

        // 转换考试为Course结构
        let mut exams = Vec::new();
        for exam in exam_response.data {
            let course = self.convert_exam_to_course(&exam)?;
            exams.push(course);
        }

        Ok((exams, exam_response.now_week))
    }

    /// 将课程转换为带重复规则的Course结构
    fn convert_class_to_course_with_recurrence(
        &self,
        class: &RedrockClass,
        base_date: &DateTime<FixedOffset>,
    ) -> Result<Course> {
        // 计算第一次上课时间（取第一个上课周）
        let first_week = *class
            .week
            .first()
            .ok_or_else(|| self.base.custom_error("Course has no week data"))?;

        let (start_time, end_time) = self.calculate_class_time(
            first_week,
            class.hash_day + 1,
            class.begin_lesson,
            class.period,
            base_date,
        )?;

        // 创建重复规则
        let recurrence = self.create_recurrence_rule(class, base_date)?;

        let mut extra = HashMap::new();
        extra.insert("course_num".to_string(), class.course_num.clone());
        extra.insert("raw_week".to_string(), class.raw_week.clone());
        extra.insert("weeks".to_string(), format!("{:?}", class.week));
        extra.insert("hash_day".to_string(), class.hash_day.to_string());
        extra.insert("begin_lesson".to_string(), class.begin_lesson.to_string());
        extra.insert("period".to_string(), class.period.to_string());

        Ok(Course {
            name: class.course.clone(),
            code: Some(class.course_num.clone()),
            teacher: Some(class.teacher.clone()),
            location: Some(class.classroom.clone()),
            start_time,
            end_time,
            description: Some(format!(
                "第{}-{}周 第{}-{}节",
                class.week.first().unwrap_or(&0),
                class.week.last().unwrap_or(&0),
                class.begin_lesson,
                class.begin_lesson + class.period - 1
            )),
            course_type: Some(class.course_type.clone()),
            credits: None,
            recurrence: Some(recurrence),
            extra,
        })
    }

    /// 创建重复规则
    fn create_recurrence_rule(
        &self,
        class: &RedrockClass,
        base_date: &DateTime<FixedOffset>,
    ) -> Result<RecurrenceRule> {
        // 计算学期结束时间
        let last_week = *class
            .week
            .last()
            .ok_or_else(|| self.base.custom_error("Course has no week data"))?;

        let (_, until_end_time) = self.calculate_class_time(
            last_week,
            class.hash_day + 1,
            class.begin_lesson,
            class.period,
            base_date,
        )?;

        // 检查是否是连续的周次
        let is_continuous = class.week.windows(2).all(|w| w[1] == w[0] + 1);

        let (frequency, interval, count, until, exception_dates) = if is_continuous {
            // 连续周次，使用简单的WEEKLY重复
            (
                "WEEKLY".to_string(),
                1,
                None,
                Some(until_end_time),
                Vec::new(),
            )
        } else {
            // 非连续周次，计算例外日期
            let mut exceptions = Vec::new();

            // 找出缺失的周次
            if let (Some(&first), Some(&last)) = (class.week.first(), class.week.last()) {
                for week in first..=last {
                    if !class.week.contains(&week) {
                        let (exception_start, _) = self.calculate_class_time(
                            week,
                            class.hash_day + 1,
                            class.begin_lesson,
                            class.period,
                            base_date,
                        )?;
                        exceptions.push(exception_start);
                    }
                }
            }

            (
                "WEEKLY".to_string(),
                1,
                None,
                Some(until_end_time),
                exceptions,
            )
        };

        // 确定星期几 (hash_day+1 转换为1-7的weekday)
        let by_day = vec![class.hash_day + 1];

        Ok(RecurrenceRule {
            frequency,
            interval,
            until,
            count,
            by_day: Some(by_day),
            exception_dates,
        })
    }

    /// 将考试转换为Course结构
    fn convert_exam_to_course(&self, exam: &RedrockExam) -> Result<Course> {
        // 解析考试时间 - 结合日期和时间信息
        let start_time =
            self.parse_exam_time_with_date(&exam.begin_time, &exam.week, &exam.weekday)?;
        let end_time = self.parse_exam_time_with_date(&exam.end_time, &exam.week, &exam.weekday)?;

        let mut extra = HashMap::new();
        extra.insert("exam_type".to_string(), exam.exam_type.clone());
        extra.insert("status".to_string(), exam.status.clone());
        extra.insert("week".to_string(), exam.week.clone());
        extra.insert("weekday".to_string(), exam.weekday.clone());
        if let Some(ref seat) = exam.seat {
            extra.insert("seat".to_string(), seat.clone());
        }

        let description = format!("考试 - {} ({})", exam.course, exam.status);

        Ok(Course {
            name: format!("{} (考试)", exam.course),
            code: None,
            teacher: None,
            location: Some(exam.classroom.clone()),
            start_time,
            end_time,
            description: Some(description),
            course_type: Some("考试".to_string()),
            credits: None,
            recurrence: None, // 考试不使用重复规则
            extra,
        })
    }

    /// 计算课程的具体上课时间
    fn calculate_class_time(
        &self,
        week_num: u32,
        weekday: u32,
        begin_lesson: u32,
        period: u32,
        base_date: &DateTime<FixedOffset>,
    ) -> Result<(DateTime<FixedOffset>, DateTime<FixedOffset>)> {
        // 防御性编程：如果 base_date 不是周一，自动找到该周的周一
        // base_date 已经是带时区的时间
        let base_local_date = base_date.date_naive();

        let semester_start_monday = if base_local_date.weekday() != chrono::Weekday::Mon {
            let days_since_monday = base_local_date.weekday().num_days_from_monday();
            base_local_date - chrono::Duration::days(days_since_monday as i64)
        } else {
            base_local_date
        };
        let target_week_monday =
            semester_start_monday + chrono::Duration::weeks((week_num - 1) as i64);
        let class_date = target_week_monday + chrono::Duration::days((weekday - 1) as i64);

        if begin_lesson == 0 || begin_lesson > LESSON_TIMES.len() as u32 {
            return Err(self
                .base
                .custom_error(format!("Invalid lesson number: {}", begin_lesson)));
        }

        let start_minutes = LESSON_TIMES[(begin_lesson - 1) as usize].0;
        let end_lesson = begin_lesson + period - 1;
        let end_minutes = if end_lesson <= LESSON_TIMES.len() as u32 {
            LESSON_TIMES[(end_lesson - 1) as usize].1
        } else {
            start_minutes + (period * 45) as usize // 每节课45分钟
        };

        let start_time = class_date
            .and_hms_opt((start_minutes / 60) as u32, (start_minutes % 60) as u32, 0)
            .ok_or_else(|| self.base.custom_error("Invalid start time"))?;

        let end_time = class_date
            .and_hms_opt((end_minutes / 60) as u32, (end_minutes % 60) as u32, 0)
            .ok_or_else(|| self.base.custom_error("Invalid end time"))?;

        // 重庆时间为UTC+8
        let tz = self.timezone();
        let start_dt = tz
            .from_local_datetime(&start_time)
            .single()
            .ok_or_else(|| {
                self.base
                    .custom_error("Failed to convert start time to UTC")
            })?;

        let end_dt = tz
            .from_local_datetime(&end_time)
            .single()
            .ok_or_else(|| self.base.custom_error("Failed to convert end time to UTC"))?;

        Ok((start_dt, end_dt))
    }

    fn parse_exam_time_with_date(
        &self,
        time_str: &str,
        week_str: &str,
        weekday_str: &str,
    ) -> Result<DateTime<FixedOffset>> {
        // 首先尝试原有的完整日期时间格式
        if let Ok(datetime) = self.parse_exam_time(time_str) {
            return Ok(datetime);
        }

        // 如果只是时间格式（如"19:30"），则需要构建完整的日期时间
        let time_parts: Vec<&str> = time_str.split(':').collect();
        if time_parts.len() != 2 {
            return Err(self
                .base
                .custom_error(format!("Invalid time format: {}", time_str)));
        }

        let hour: u32 = time_parts[0].parse().map_err(|_| {
            self.base
                .custom_error(format!("Invalid hour in time: {}", time_str))
        })?;
        let minute: u32 = time_parts[1].parse().map_err(|_| {
            self.base
                .custom_error(format!("Invalid minute in time: {}", time_str))
        })?;

        // 解析周数和星期
        let week_num: u32 = week_str.parse().map_err(|_| {
            self.base
                .custom_error(format!("Invalid week number: {}", week_str))
        })?;

        let weekday: u32 = weekday_str.parse().map_err(|_| {
            self.base
                .custom_error(format!("Invalid weekday: {}", weekday_str))
        })?;

        // 使用与课程相同的逻辑计算日期（假设学期开始时间）
        // 这里使用一个假设的学期开始日期，实际应该从配置或其他地方获取
        let base_date = chrono::NaiveDate::from_ymd_opt(2024, 3, 4) // 假设2024年春季学期开始
            .ok_or_else(|| self.base.custom_error("Invalid base date"))?;

        let days_since_monday = base_date.weekday().num_days_from_monday();
        let monday = base_date - chrono::Duration::days(days_since_monday as i64);
        let target_week_monday = monday + chrono::Duration::weeks((week_num - 1) as i64);
        let exam_date = target_week_monday + chrono::Duration::days((weekday - 1) as i64);

        // 构建完整的考试时间
        let naive_datetime = exam_date
            .and_hms_opt(hour, minute, 0)
            .ok_or_else(|| self.base.custom_error("Failed to create exam datetime"))?;

        // 转换为UTC时间 (重庆时间为UTC+8)
        let tz = self.timezone();
        let dt = tz
            .from_local_datetime(&naive_datetime)
            .single()
            .ok_or_else(|| self.base.custom_error("Failed to convert exam time to UTC"))?;

        Ok(dt)
    }

    fn parse_exam_time(&self, time_str: &str) -> Result<DateTime<FixedOffset>> {
        // 尝试解析时间格式，例如 "2024-01-15 14:00:00"
        let naive_datetime = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M"))
            .map_err(|_| {
                self.base
                    .custom_error(format!("Failed to parse exam time: {}", time_str))
            })?;

        // 转换为UTC时间 (假设重庆时间为UTC+8)
        let tz = self.timezone();
        let dt = tz
            .from_local_datetime(&naive_datetime)
            .single()
            .ok_or_else(|| self.base.custom_error("Failed to convert exam time to UTC"))?;

        Ok(dt)
    }
}

#[async_trait]
impl Provider for RedrockProvider {
    type Token = RedrockToken;

    fn name(&self) -> &str {
        &self.base.info.name
    }

    fn description(&self) -> &str {
        &self.base.info.description
    }

    fn timezone(&self) -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }

    async fn authenticate(&self, request: &CourseRequest) -> Result<Self::Token> {
        tracing::info!(
            "Getting credentials for redrock user: {}",
            request.credentials.username
        );

        let token = self.authenticate(&request.credentials).await?;
        Ok(token)
    }

    async fn validate_token(&self, token: &Self::Token) -> Result<bool> {
        // 检查token的状态字段
        Ok(token.status == "10000" && !token.data.token.is_empty())
    }

    async fn get_courses(
        &self,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<CourseResponse> {
        // 验证token
        if !self.validate_token(token).await? {
            return Err(self.base.custom_error("Invalid or expired token"));
        }

        tracing::info!(
            "Fetching courses from redrock for user: {}",
            request.credentials.username
        );

        // 获取课程表数据
        let (courses, current_week) =
            self.get_class_schedule(request, token).await.map_err(|e| {
                tracing::error!("Failed to get class schedule: {}", e);
                e
            })?;

        // 获取考试安排
        let (exams, _) = self
            .get_exam_schedule(&request.credentials.username)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to get exam schedule: {}", e);
                // 考试安排获取失败不影响课程表，只记录警告
                e
            })
            .unwrap_or_else(|_| (Vec::new(), 0));
        // 合并课程和考试
        let mut all_courses = courses;
        all_courses.extend(exams);

        tracing::info!(
            "Successfully fetched {} courses/exams from redrock (current week: {})",
            all_courses.len(),
            current_week
        );

        Ok(CourseResponse {
            courses: all_courses,
            semester: request.semester.clone().unwrap(),
            generated_at: Utc::now().with_timezone(&self.timezone()),
        })
    }

    async fn refresh_token(&self, token: &Self::Token) -> Result<Self::Token> {
        // TODO: 实现token刷新逻辑
        Ok(token.clone())
    }

    async fn logout(&self, token: &Self::Token) -> Result<()> {
        // Redrock provider doesn't require explicit logout
        let _ = token; // 避免未使用参数警告
        Ok(())
    }

    fn token_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(3600 * 24 * 3)
    }
}
