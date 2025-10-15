use chrono::{DateTime, FixedOffset, Utc};
use std::borrow::Cow;
use uuid::Uuid;

use crate::{
    Course, CourseResponse, Error, IcsOptions, RecurrenceRule, Result, location::LocationManager,
};

/// ICS日历生成器
pub struct IcsGenerator {
    options: IcsOptions,
    location_manager: LocationManager,
}

impl IcsGenerator {
    pub fn new(options: IcsOptions) -> Self {
        Self {
            options,
            location_manager: LocationManager::default(),
        }
    }

    /// 生成ICS日历内容
    pub fn generate(&self, response: &CourseResponse) -> Result<String> {
        // 首先处理课程，智能创建重复规则
        let processed_courses = self.process_courses(&response.courses)?;

        let mut ics_content = String::new();

        // ICS文件头部
        ics_content.push_str("BEGIN:VCALENDAR\r\n");
        ics_content.push_str("VERSION:2.0\r\n");
        ics_content.push_str("PRODID:-//CQUPT ICS//CQUPT Course Calendar//CN\r\n");
        ics_content.push_str("CALSCALE:GREGORIAN\r\n");
        ics_content.push_str("METHOD:PUBLISH\r\n");

        if let Some(ref name) = self.options.calendar_name {
            ics_content.push_str(&format!("X-WR-CALNAME:{}\r\n", name));
        }

        // 添加课程事件
        for course_with_recurrence in &processed_courses {
            self.add_course_event(&mut ics_content, course_with_recurrence)?;
        }

        // ICS文件尾部
        ics_content.push_str("END:VCALENDAR\r\n");

        Ok(ics_content)
    }

    /// 处理课程列表，智能创建重复规则
    fn process_courses(&self, courses: &[Course]) -> Result<Vec<CourseWithRecurrence>> {
        let mut processed = Vec::new();

        for course in courses {
            let processed_course = if self.is_exam_course(course) {
                // 考试不需要重复规则
                CourseWithRecurrence {
                    course: course.clone(),
                    recurrence: None,
                }
            } else if let (Some(weeks), Some(weekday)) = (&course.weeks, course.weekday) {
                // 创建重复规则
                let recurrence =
                    self.create_recurrence_rule(Cow::Borrowed(weeks), weekday, &course.start_time)?;

                CourseWithRecurrence {
                    course: course.clone(),
                    recurrence: Some(recurrence),
                }
            } else {
                // 没有足够信息创建重复规则，作为单次事件
                CourseWithRecurrence {
                    course: course.clone(),
                    recurrence: None,
                }
            };

            processed.push(processed_course);
        }

        Ok(processed)
    }

    /// 判断是否是考试课程
    fn is_exam_course(&self, course: &Course) -> bool {
        course.exam_type.is_some()
            || course
                .course_type
                .as_ref()
                .is_some_and(|t| t.contains("考试"))
    }

    /// 创建重复规则
    fn create_recurrence_rule(
        &self,
        mut weeks: Cow<[u32]>,
        weekday: u32,
        start_time: &DateTime<FixedOffset>,
    ) -> Result<RecurrenceRule> {
        if weeks.is_empty() {
            return Err(Error::Config("Course has no week data".to_string()));
        }
        if !weeks.is_sorted() {
            weeks.to_mut().sort();
        }
        // 计算学期结束时间（最后一周的课程结束时间）
        let last_week = *weeks.last().unwrap();
        let weeks_duration = chrono::Duration::weeks(last_week as i64 - 1);
        let until_end_time = *start_time + weeks_duration;

        // 检查是否间隔n周
        let is_continuous = weeks.len() > 1 && {
            let gap = weeks[1] - weeks[0];
            weeks.windows(2).all(|w| w[1] == w[0] + gap)
        };

        let (frequency, interval, count, until, exception_dates) = if is_continuous {
            // 连续周次，使用简单的WEEKLY重复
            (
                "WEEKLY".to_string(),
                weeks[1] - weeks[0],
                None,
                Some(until_end_time),
                Vec::new(),
            )
        } else {
            // 非连续周次，计算例外日期
            let mut exceptions = Vec::new();

            // 找出缺失的周次
            if let (Some(&first), Some(&last)) = (weeks.first(), weeks.last()) {
                for week in first..=last {
                    if !weeks.contains(&week) {
                        // 计算这一周的课程时间作为例外日期
                        let weeks_offset = chrono::Duration::weeks((week - first) as i64);
                        let exception_time = *start_time + weeks_offset;
                        exceptions.push(exception_time);
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

        Ok(RecurrenceRule {
            frequency,
            interval,
            until,
            count,
            by_day: Some(vec![weekday]),
            exception_dates,
        })
    }

    /// 添加单个课程事件
    fn add_course_event(
        &self,
        ics_content: &mut String,
        course_with_recurrence: &CourseWithRecurrence,
    ) -> Result<()> {
        let course = &course_with_recurrence.course;
        let uid = Uuid::new_v4().to_string();
        let dtstamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();

        // 根据 ICS 标准，DateTime<FixedOffset> 应该转换为 UTC 格式
        // 这样既符合标准，又充分利用了 FixedOffset 的时区信息
        let dtstart_utc = course.start_time.to_utc();
        let dtend_utc = course.end_time.to_utc();
        let dtstart = dtstart_utc.format("%Y%m%dT%H%M%SZ").to_string();
        let dtend = dtend_utc.format("%Y%m%dT%H%M%SZ").to_string();

        ics_content.push_str("BEGIN:VEVENT\r\n");
        ics_content.push_str(&format!("UID:{}\r\n", uid));
        ics_content.push_str(&format!("DTSTAMP:{}\r\n", dtstamp));
        ics_content.push_str(&format!("DTSTART:{}\r\n", dtstart));
        ics_content.push_str(&format!("DTEND:{}\r\n", dtend));
        ics_content.push_str(&format!(
            "SUMMARY:{}\r\n",
            self.escape_text(&self.build_course_title(course))
        ));

        // 添加位置信息（包含地理坐标）
        if let Some(ref location) = course.location {
            let location_with_geo = self.location_manager.get_location_with_geo(location);
            ics_content.push_str(&location_with_geo);
        }

        // 构建描述信息
        if self.options.include_description {
            let description = self.build_course_description(course);
            ics_content.push_str(&format!(
                "DESCRIPTION:{}\r\n",
                self.escape_text(&description)
            ));
        }

        // 添加提醒
        if let Some(reminder_minutes) = self.options.reminder_minutes {
            ics_content.push_str("BEGIN:VALARM\r\n");
            ics_content.push_str("ACTION:DISPLAY\r\n");
            ics_content.push_str("DESCRIPTION:课程提醒\r\n");
            ics_content.push_str(&format!("TRIGGER:-PT{}M\r\n", reminder_minutes));
            ics_content.push_str("END:VALARM\r\n");
        }

        // 添加重复规则
        if let Some(ref recurrence) = course_with_recurrence.recurrence {
            self.add_recurrence_rule(ics_content, recurrence)?;
        }

        ics_content.push_str("END:VEVENT\r\n");

        Ok(())
    }

    /// 转义ICS文本内容
    fn escape_text(&self, text: &str) -> String {
        text.replace("\\", "\\\\")
            .replace("\n", "\\n")
            .replace("\r", "\\r")
            .replace(",", "\\,")
            .replace(";", "\\;")
    }

    /// 构建课程描述信息
    pub fn build_course_description(&self, course: &Course) -> String {
        // 检查是否是考试类型
        if self.is_exam_course(course) {
            self.build_exam_description(course)
        } else {
            self.build_class_description(course)
        }
    }

    /// 构建课程标题
    pub fn build_course_title(&self, course: &Course) -> String {
        if self.is_exam_course(course) {
            // 考试类型：[考试类型考试] 课程名 - 地点
            let exam_type = course.exam_type.as_deref().unwrap_or("");
            let location = course.location.as_deref().unwrap_or("");
            format!("[{}考试] {} - {}", exam_type, course.name, location)
        } else {
            // 普通课程：课程名 - 地点
            if let Some(location) = course.location.as_ref().filter(|l| !l.is_empty()) {
                let location = self.location_manager.get_location_with_geo(location);
                format!("{} - {}", course.name, location)
            } else {
                course.name.to_string()
            }
        }
    }

    /// 构建普通课程描述
    pub fn build_class_description(&self, course: &Course) -> String {
        let mut segments = Vec::new();

        if let Some(code) = course.code.as_ref().filter(|c| !c.is_empty()) {
            segments.push(code.to_string());
        }

        if let Some(teacher) = course.teacher.as_ref().filter(|t| !t.is_empty()) {
            segments.push(format!("任课教师: {}", teacher));
        }

        if let Some(course_type) = course.course_type.as_ref().filter(|t| !t.is_empty()) {
            segments.push(format!("该课程是{}课", course_type));
        }

        if let Some(desc) = course.description.as_ref().filter(|d| !d.is_empty()) {
            segments.push(desc.to_string());
        }

        if segments.is_empty() {
            "暂无课程详情".to_string()
        } else {
            segments.join("，")
        }
    }

    /// 构建考试描述
    pub fn build_exam_description(&self, course: &Course) -> String {
        // 获取考试相关信息
        let seat = course.seat.as_deref().unwrap_or("待定");
        let status = course.status.as_deref().unwrap_or("");
        let week = course.raw_week.as_deref().unwrap_or("");

        let test_status = if status.is_empty() { "正常" } else { status };

        // 格式化考试时间
        let start_time = course.start_time.format("%H:%M").to_string();
        let end_time = course.end_time.format("%H:%M").to_string();

        let current_week = if !week.is_empty() {
            week.to_string()
        } else {
            "未知".to_string()
        };

        format!(
            "考试在第{}周进行，时间为{}至{}，考试座位号是{}，考试状态: {}，祝考试顺利！（最终考试信息请以教务在线公布为准）",
            current_week, start_time, end_time, seat, test_status
        )
    }

    /// 添加重复规则
    fn add_recurrence_rule(
        &self,
        ics_content: &mut String,
        recurrence: &RecurrenceRule,
    ) -> Result<()> {
        let mut rrule = format!("RRULE:FREQ={}", recurrence.frequency);

        if recurrence.interval > 1 {
            rrule.push_str(&format!(";INTERVAL={}", recurrence.interval));
        }

        if let Some(until) = recurrence.until {
            // 根据 ICS 标准，UNTIL 必须与 DTSTART 使用相同格式
            let until_utc = until.to_utc();
            rrule.push_str(&format!(";UNTIL={}", until_utc.format("%Y%m%dT%H%M%SZ")));
        }

        if let Some(count) = recurrence.count {
            rrule.push_str(&format!(";COUNT={}", count));
        }

        if let Some(ref by_day) = recurrence.by_day {
            let days: Vec<String> = by_day
                .iter()
                .map(|d| match d {
                    1 => "MO".to_string(),
                    2 => "TU".to_string(),
                    3 => "WE".to_string(),
                    4 => "TH".to_string(),
                    5 => "FR".to_string(),
                    6 => "SA".to_string(),
                    7 => "SU".to_string(),
                    _ => format!("{}", d),
                })
                .collect();
            if !days.is_empty() {
                rrule.push_str(&format!(";BYDAY={}", days.join(",")));
            }
        }

        ics_content.push_str(&format!("{}\r\n", rrule));

        // 添加例外日期
        for exception_date in &recurrence.exception_dates {
            // 转换为 UTC 格式以保持一致性
            let exception_utc = exception_date.to_utc();
            ics_content.push_str(&format!(
                "EXDATE:{}\r\n",
                exception_utc.format("%Y%m%dT%H%M%SZ")
            ));
        }

        Ok(())
    }
}

/// 带重复规则的课程
#[derive(Debug, Clone)]
struct CourseWithRecurrence {
    course: Course,
    recurrence: Option<RecurrenceRule>,
}

impl Default for IcsGenerator {
    fn default() -> Self {
        Self::new(IcsOptions::default())
    }
}

#[test]
fn test_rrule_generation() {
    use chrono::{FixedOffset, TimeZone};
    let generator = IcsGenerator::default();

    let start_time = FixedOffset::east_opt(8 * 3600)
        .unwrap()
        .with_ymd_and_hms(2024, 9, 2, 10, 0, 0)
        .unwrap();

    // 测试连续周次
    let weekss = vec![vec![1, 3, 5, 7, 9], vec![2, 4]];
    for weeks in weekss {
        let recurrence = generator
            .create_recurrence_rule(Cow::Owned(weeks), 1, &start_time)
            .unwrap();
        assert_eq!(recurrence.frequency, "WEEKLY");
        assert_eq!(recurrence.interval, 2);
        assert!(recurrence.until.is_some());
        assert!(recurrence.count.is_none());
        assert_eq!(recurrence.by_day, Some(vec![1]));
        assert!(recurrence.exception_dates.is_empty());
    }

    // 测试非连续周次
    let weeks = vec![1, 2, 4, 5, 7];
    let recurrence = generator
        .create_recurrence_rule(Cow::Owned(weeks), 1, &start_time)
        .unwrap();
    assert_eq!(recurrence.frequency, "WEEKLY");
    assert_eq!(recurrence.interval, 1);
    assert!(recurrence.until.is_some());
    assert!(recurrence.count.is_none());
    assert_eq!(recurrence.by_day, Some(vec![1]));
    assert_eq!(recurrence.exception_dates.len(), 2); // 第3和第6周缺失
}
