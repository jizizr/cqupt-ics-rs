use std::collections::HashMap;

use crate::{
    Course, CourseRequest, CourseResponse, Error, Result,
    prelude::*,
    providers::{BaseProvider, ParamContext, ParamContextExt, Provider},
};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDateTime, TimeZone, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

const LESSON_TIMES: [(usize, usize); 12] = [
    (8 * 60, 8 * 60 + 45),        // 第1节: 08:00-08:45
    (8 * 60 + 55, 9 * 60 + 40),   // 第2节: 08:55-09:40
    (10 * 60 + 15, 11 * 60),      // 第3节: 10:15-11:00
    (11 * 60 + 55, 11 * 60 + 55), // 第4节: 11:15-11:55
    (14 * 60, 14 * 60 + 45),      // 第5节: 14:00-14:45
    (14 * 60 + 55, 15 * 60 + 40), // 第6节: 14:55-15:40
    (16 * 60 + 15, 17 * 60),      // 第7节: 16:15-17:00
    (17 * 60 + 10, 17 * 60 + 55), // 第8节: 17:10-17:55
    (19 * 60, 19 * 60 + 45),      // 第9节: 19:00-19:45
    (19 * 60 + 55, 20 * 60 + 40), // 第10节: 19:55-20:40
    (20 * 60 + 50, 21 * 60 + 35), // 第11节: 20:50-21:35
    (21 * 60 + 45, 22 * 60 + 30), // 第12节: 21:45-22:30
];

/// Redrock API响应数据结构
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RedrockResponse {
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
#[derive(Debug, Clone, Deserialize, Default)]
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

#[derive(Debug, Deserialize)]
struct RedrockCustomScheduleResponse {
    status: u32,
    data: Vec<RedrockCustomSchedule>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct RedrockCustomSchedule {
    id: u32,
    time: u32,
    title: String,
    content: String,
    date: Vec<RedrockCustomScheduleDate>,
}

#[derive(Debug, Deserialize)]
struct RedrockCustomScheduleDate {
    begin_lesson: u32,
    period: u32,
    day: u32,
    week: Vec<u32>,
}

pub struct RedrockProvider {
    base: BaseProvider,
}

impl RedrockProvider {
    const API_ROOT: &'static str = "https://be-prod.redrock.cqupt.edu.cn";
    pub fn new() -> Self {
        let mut base = BaseProviderBuilder::new(ProviderInfo {
            name: "redrock".to_string(),
            description: "掌上重邮 API".to_string(),
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
        let days_since_monday = now_local.weekday().num_days_from_monday() as i64;

        let weeks_back = (now_week - 1) as i64;
        let total_days_back = days_since_monday + weeks_back * 7;

        // 计算学期开始的那天的午夜
        let days_to_subtract = chrono::Duration::days(total_days_back);
        let semester_start_day = now_local - days_to_subtract;

        // 设置为那天的午夜 (00:00:00)
        let start_local = semester_start_day
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| {
                self.base
                    .custom_error("failed to create midnight".to_string())
            })?;

        let start_local = tz
            .from_local_datetime(&start_local)
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
            if response.status() == StatusCode::INTERNAL_SERVER_ERROR {
                return Err(crate::Error::CurfewTime(()));
            } else {
                return Err(self
                    .base
                    .custom_error(format!("HTTP {} error", response.status())));
            }
        }

        response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse response: {}", e))
        })
    }

    /// 获取自定义日程
    async fn get_custom_schedule_data(
        &self,
        token: &RedrockToken,
    ) -> Result<RedrockCustomScheduleResponse> {
        let url = format!("{}/magipoke-reminder/Person/getTransaction", Self::API_ROOT);

        let response = self
            .base
            .client
            .post(&url)
            .header("App-Version", "74")
            .bearer_auth(&token.data.token)
            .send()
            .await
            .map_err(|e| self.base.handle_error_req(e))?;

        if !response.status().is_success() {
            return Err(self
                .base
                .custom_error(format!("HTTP {} error", response.status())));
        }

        let r: RedrockCustomScheduleResponse = response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse response: {}", e))
        })?;

        if r.status != 200 {
            return Err(self
                .base
                .custom_error(format!("API returned error status: {}", r.status)));
        }

        Ok(r)
    }

    /// 获取课程表数据
    async fn get_class_schedule(
        &self,
        context: &mut Context<RedrockResponse>,
        request: &mut CourseRequest,
        token: &RedrockToken,
    ) -> Result<(Vec<Course>, u32)> {
        let start_date = request
            .semester
            .as_ref()
            .ok_or_else(|| {
                self.base
                    .custom_error("Semester start date is required".to_string())
            })?
            .start_date;

        let mut courses = Vec::new();
        let redrock_response = match context.as_ref() {
            Some(data) => data,
            None => {
                &self
                    .get_class_schedule_data(&request.credentials.username, token)
                    .await?
            }
        };
        for class in &redrock_response.data {
            let course =
                self.convert_class_to_course(class, &start_date, redrock_response.now_week)?;
            courses.push(course);
        }

        Ok((courses, redrock_response.now_week))
    }

    /// 获取考试安排
    async fn get_exam_schedule(
        &self,
        student_id: &str,
        semester_start: &DateTime<FixedOffset>,
    ) -> Result<(Vec<Course>, u32)> {
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
            let course = self.convert_exam_to_course(&exam, semester_start)?;
            exams.push(course);
        }

        Ok((exams, exam_response.now_week))
    }

    async fn get_custom_schedule(
        &self,
        request: &mut CourseRequest,
        token: &RedrockToken,
    ) -> Result<Vec<Course>> {
        let start_date = request
            .semester
            .as_ref()
            .ok_or_else(|| {
                self.base
                    .custom_error("Semester start date is required".to_string())
            })?
            .start_date;
        let custom_response = self.get_custom_schedule_data(token).await?;
        let mut courses = Vec::new();
        for custom in &custom_response.data {
            let custom_courses = self.convert_custom_schedule_to_course(custom, &start_date, 0)?;
            courses.extend(custom_courses);
        }
        Ok(courses)
    }

    /// 将课程转换为Course结构
    fn convert_class_to_course(
        &self,
        class: &RedrockClass,
        base_date: &DateTime<FixedOffset>,
        current_week: u32,
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

        Ok(Course {
            name: class.course.clone(),
            code: Some(class.course_num.clone()),
            teacher: Some(class.teacher.clone()),
            location: Some(class.classroom.clone()),
            start_time,
            end_time,
            course_type: Some(class.course_type.clone()),

            // 提供原始数据供 ICS 模块使用
            weeks: Some(class.week.clone()),
            weekday: Some(class.hash_day + 1), // 转换为1-7格式
            begin_lesson: Some(class.begin_lesson),
            lesson_duration: Some(class.period),

            // 显示相关字段
            raw_week: Some(class.raw_week.clone()),
            current_week: Some(current_week),

            ..Default::default()
        })
    }

    /// 将考试转换为Course结构
    fn convert_exam_to_course(
        &self,
        exam: &RedrockExam,
        semester_start: &DateTime<FixedOffset>,
    ) -> Result<Course> {
        // 解析考试时间 - 结合日期和时间信息
        let start_time = self.parse_exam_time_with_date(
            &exam.begin_time,
            &exam.week,
            &exam.weekday,
            semester_start,
        )?;
        let end_time = self.parse_exam_time_with_date(
            &exam.end_time,
            &exam.week,
            &exam.weekday,
            semester_start,
        )?;

        Ok(Course {
            name: format!("{} (考试)", exam.course),
            location: Some(exam.classroom.clone()),
            start_time,
            end_time,
            course_type: Some("考试".to_string()),

            // 考试相关字段
            exam_type: Some(exam.exam_type.clone()),
            seat: exam.seat.clone(),
            status: Some(exam.status.clone()),
            raw_week: Some(exam.week.clone()),
            ..Default::default()
        })
    }

    fn convert_custom_schedule_to_course(
        &self,
        custom: &RedrockCustomSchedule,
        base_date: &DateTime<FixedOffset>,
        current_week: u32,
    ) -> Result<Vec<Course>> {
        let mut courses = Vec::with_capacity(custom.date.len());
        for item in &custom.date {
            let (start_time, end_time) = self.calculate_class_time(
                item.week.first().copied().unwrap_or(1),
                item.day + 1,
                item.begin_lesson,
                item.period,
                base_date,
            )?;
            courses.push(Course {
                name: custom.title.clone(),
                code: Some(custom.id.to_string()),

                start_time,
                end_time,
                note: Some(format!("自定义日程: {}", custom.content)),

                // 提供原始数据供 ICS 模块使用
                weeks: Some(item.week.clone()),
                weekday: Some(item.day),
                begin_lesson: Some(item.begin_lesson),
                lesson_duration: Some(item.period),
                current_week: Some(current_week),

                ..Default::default()
            });
        }
        Ok(courses)
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
        // 直接使用DateTime<FixedOffset>计算日期
        let days_since_monday = base_date.weekday().num_days_from_monday();
        let semester_start_monday = if base_date.weekday() != chrono::Weekday::Mon {
            *base_date - chrono::Duration::days(days_since_monday as i64)
        } else {
            *base_date
        };
        let target_week_monday =
            semester_start_monday + chrono::Duration::weeks((week_num - 1) as i64);
        let class_date_base = target_week_monday + chrono::Duration::days((weekday - 1) as i64);

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

        // 直接在DateTime<FixedOffset>基础上加时间
        let start_dt = class_date_base
            + chrono::Duration::hours((start_minutes / 60) as i64)
            + chrono::Duration::minutes((start_minutes % 60) as i64);

        let end_dt = class_date_base
            + chrono::Duration::hours((end_minutes / 60) as i64)
            + chrono::Duration::minutes((end_minutes % 60) as i64);

        Ok((start_dt, end_dt))
    }

    fn parse_exam_time_with_date(
        &self,
        time_str: &str,
        week_str: &str,
        weekday_str: &str,
        semester_start: &DateTime<FixedOffset>,
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

        // 使用实际的学期开始日期来计算考试日期
        let days_since_monday = semester_start.weekday().num_days_from_monday();
        let monday = *semester_start - chrono::Duration::days(days_since_monday as i64);
        let target_week_monday = monday + chrono::Duration::weeks((week_num - 1) as i64);
        let exam_date_base = target_week_monday + chrono::Duration::days((weekday - 1) as i64);

        // 直接在现有日期时间基础上设置时分秒
        let dt = exam_date_base
            + chrono::Duration::hours(hour as i64)
            + chrono::Duration::minutes(minute as i64);

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
    type ContextType = RedrockResponse;
    fn name(&self) -> &str {
        &self.base.info.name
    }

    fn description(&self) -> &str {
        &self.base.info.description
    }

    fn timezone(&self) -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }

    async fn authenticate<'a>(
        &'a self,
        _context: ParamContext<'_, Self::ContextType>,
        request: &CourseRequest,
    ) -> Result<Self::Token> {
        tracing::info!(
            "Getting credentials for redrock user: {}",
            request.credentials.username
        );
        let credentials = &request.credentials;
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
        if response.status() == StatusCode::BAD_REQUEST {
            return Err(crate::Error::Authentication("密码错误".to_string()));
        }
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

    async fn validate_token(&self, token: &Self::Token) -> Result<bool> {
        // 检查token的状态字段
        Ok(token.status == "10000"
            && !token.data.token.is_empty()
            && !base::is_token_expired(&token.data.token)?)
    }

    async fn get_semester_start<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<chrono::DateTime<FixedOffset>> {
        let ctx = context.ensure_valid()?;

        let redrock_response = match ctx.as_ref() {
            Some(data) => data,
            None => {
                let data = self
                    .get_class_schedule_data(&request.credentials.username, token)
                    .await?;
                ctx.set(data);
                // 现在获取刚设置的数据的引用
                ctx.as_ref().ok_or(
                    self.base
                        .custom_error("Failed to get RedrockResponse from context".to_string()),
                )?
            }
        };

        if redrock_response.now_week == 0 {
            // 如果now_week为0，尝试从version字段解析学期开始时间
            self.parse_semester_start_from_version(&redrock_response.version)
        } else {
            // 使用now_week计算学期开始时间
            self.get_semester_start_from_now_week(redrock_response.now_week)
        }
    }

    async fn get_courses<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<CourseResponse> {
        let ctx = context.ensure_valid()?;
        // 验证token
        if !self.validate_token(token).await? {
            return Err(self.base.custom_error("Invalid or expired token"));
        }

        tracing::info!(
            "Fetching courses from redrock for user: {}",
            request.credentials.username
        );

        let (courses, current_week) =
            self.get_class_schedule(ctx, request, token)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to get class schedule: {}", e);
                    e
                })?;

        let semester_start = &request.semester.as_ref().unwrap().start_date;
        let (exams, _) = self
            .get_exam_schedule(&request.credentials.username, semester_start)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to get exam schedule: {}", e);
                e
            })
            .unwrap_or_else(|_| (Vec::new(), 0));

        let custom_courses = self
            .get_custom_schedule(request, token)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to get custom schedule: {}", e);
                Vec::new()
            });
        // 合并课程和考试
        let mut all_courses = courses;
        all_courses.extend(exams);
        all_courses.extend(custom_courses);

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
        tracing::info!("Refreshing token for redrock");
        let url = format!("{}/magipoke/token/refresh", Self::API_ROOT);

        let mut data = HashMap::new();
        data.insert("refreshToken", &token.data.refresh_token);

        let response = self
            .base
            .client
            .post(&url)
            .header("Host", Self::API_ROOT.trim_start_matches("https://"))
            .header("Accept", "*/*")
            .header("Connection", "keep-alive")
            .bearer_auth(&token.data.token)
            .header("Content-Type", "application/json")
            .json(&data)
            .send()
            .await
            .map_err(|e| self.base.handle_error_req(e))?;

        if !response.status().is_success() {
            return Err(self.base.custom_error(format!(
                "HTTP {} error when refreshing token",
                response.status()
            )));
        }

        let refreshed_token: RedrockToken = response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse refresh token response: {}", e))
        })?;

        // 验证刷新后的token状态
        if self.validate_token(&refreshed_token).await? {
            return Err(self
                .base
                .custom_error("Refresh token returned invalid status"));
        }

        Ok(refreshed_token)
    }

    fn token_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(3600 * 24 * 3)
    }
}
