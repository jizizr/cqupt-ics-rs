use chrono::Utc;
use uuid::Uuid;

use crate::{
    Course, CourseResponse, IcsOptions, RecurrenceRule, Result, location::LocationManager,
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

        if let Some(ref timezone) = self.options.timezone {
            ics_content.push_str(&format!("X-WR-TIMEZONE:{}\r\n", timezone));
        }

        // 添加课程事件
        for course in &response.courses {
            self.add_course_event(&mut ics_content, course)?;
        }

        // ICS文件尾部
        ics_content.push_str("END:VCALENDAR\r\n");

        Ok(ics_content)
    }

    /// 添加单个课程事件
    fn add_course_event(&self, ics_content: &mut String, course: &Course) -> Result<()> {
        let uid = Uuid::new_v4().to_string();
        let dtstamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let dtstart = course.start_time.format("%Y%m%dT%H%M%SZ").to_string();
        let dtend = course.end_time.format("%Y%m%dT%H%M%SZ").to_string();

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
        if let Some(ref recurrence) = course.recurrence {
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
        if course.course_type.as_deref() == Some("考试") {
            self.build_exam_description(course)
        } else {
            self.build_class_description(course)
        }
    }

    /// 构建课程标题
    pub fn build_course_title(&self, course: &Course) -> String {
        if course.course_type.as_deref() == Some("考试") {
            // 考试类型：[考试类型考试] 课程名 - 地点
            let exam_type = course
                .extra
                .get("exam_type")
                .map(|s| s.as_str())
                .unwrap_or("");
            let location = course.location.as_deref().unwrap_or("");
            format!("[{}考试] {} - {}", exam_type, course.name, location)
        } else {
            // 普通课程：课程名 - 地点
            let location = course.location.as_deref().unwrap_or("");
            format!("{} - {}", course.name, location)
        }
    }

    /// 构建普通课程描述
    pub fn build_class_description(&self, course: &Course) -> String {
        let course_id = course.code.as_deref().unwrap_or("未知");
        let teacher = course.teacher.as_deref().unwrap_or("未知");
        let course_type = course.course_type.as_deref().unwrap_or("未知");

        // 从extra字段获取上课周次信息
        let raw_week = course
            .extra
            .get("raw_week")
            .map(|s| s.as_str())
            .unwrap_or("");

        // 获取当前周数，如果没有可以从extra或其他地方获取
        let current_week = course
            .extra
            .get("current_week")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        let formatted_weeks = if raw_week.is_empty() {
            "全学期".to_string()
        } else {
            raw_week.replace(',', "、")
        };

        format!(
            "{} 任课教师: {}，该课程是{}课，在{}行课，当前是第{}周。",
            course_id, teacher, course_type, formatted_weeks, current_week
        )
    }

    /// 构建考试描述
    pub fn build_exam_description(&self, course: &Course) -> String {
        // 从extra字段获取考试相关信息
        let seat = course
            .extra
            .get("seat")
            .map(|s| s.as_str())
            .unwrap_or("待定");
        let status = course.extra.get("status").map(|s| s.as_str()).unwrap_or("");
        let week = course.extra.get("week").map(|s| s.as_str()).unwrap_or("");

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
            rrule.push_str(&format!(";UNTIL={}", until.format("%Y%m%dT%H%M%SZ")));
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
            ics_content.push_str(&format!(
                "EXDATE:{}\r\n",
                exception_date.format("%Y%m%dT%H%M%SZ")
            ));
        }

        Ok(())
    }
}

impl Default for IcsGenerator {
    fn default() -> Self {
        Self::new(IcsOptions::default())
    }
}
