use std::collections::HashMap;

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};

/// 课程重复规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceRule {
    /// 重复频率 (WEEKLY, DAILY等)
    pub frequency: String,
    /// 重复间隔 (每N周/天)
    pub interval: u32,
    /// 结束日期 (UNTIL)
    pub until: Option<DateTime<Utc>>,
    /// 重复次数 (COUNT)
    pub count: Option<u32>,
    /// 星期几 (BYDAY) - 1=Monday, 7=Sunday
    pub by_day: Option<Vec<u32>>,
    /// 例外日期 (EXDATE)
    pub exception_dates: Vec<DateTime<Utc>>,
}

/// 课程信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Course {
    /// 课程名称
    pub name: String,
    /// 课程代码
    pub code: Option<String>,
    /// 教师姓名
    pub teacher: Option<String>,
    /// 上课地点
    pub location: Option<String>,
    /// 开始时间 (第一次上课的时间)
    pub start_time: DateTime<Utc>,
    /// 结束时间 (第一次上课的结束时间)
    pub end_time: DateTime<Utc>,
    /// 课程描述
    pub description: Option<String>,
    pub course_type: Option<String>,
    /// 学分
    pub credits: Option<f32>,
    /// 重复规则 (用于生成RRULE)
    pub recurrence: Option<RecurrenceRule>,
    /// 额外属性
    pub extra: HashMap<String, String>,
}

/// 学期信息
/// 简化设计：只需要学期开始日期，其他信息都可以推算
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Semester {
    pub start_date: DateTime<Utc>,
}

impl Semester {
    pub fn from_date_str(date_str: &str) -> Result<Self, String> {
        use chrono::{Datelike, NaiveDate, TimeZone};

        let naive_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|e| {
            format!(
                "Invalid date format '{}': {}. Expected format: YYYY-MM-DD",
                date_str, e
            )
        })?;

        // 找到这一周的星期一
        let days_since_monday = naive_date.weekday().num_days_from_monday();
        let first_monday = naive_date - chrono::Duration::days(days_since_monday as i64);

        // 转换为UTC时间（假设输入是本地时间）
        let start_datetime = first_monday
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| "Failed to create datetime".to_string())?;

        let start_date = chrono::Utc
            .from_local_datetime(&start_datetime)
            .single()
            .ok_or_else(|| "Failed to convert to UTC".to_string())?;

        Ok(Self { start_date })
    }

    /// 获取指定周数的星期一日期
    pub fn get_week_start(&self, week: u32) -> DateTime<Utc> {
        self.start_date + chrono::Duration::weeks(week as i64 - 1)
    }

    /// 获取学期开始的年份
    pub fn year(&self) -> i32 {
        self.start_date.year()
    }

    /// 推算学期类型（春季/秋季）
    pub fn term_type(&self) -> &'static str {
        let month = self.start_date.month();
        if (2..=7).contains(&month) {
            "春季"
        } else {
            "秋季"
        }
    }
}

/// 用户凭据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    /// 用户名/学号
    pub username: String,
    /// 密码
    pub password: String,
    /// 额外的认证信息
    pub extra: HashMap<String, String>,
}

/// provider配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// provider名称
    pub name: String,
    /// 基础URL
    pub base_url: String,
    pub timeout: Option<u64>,
    /// 额外配置
    pub extra: HashMap<String, String>,
}

/// 课程查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseRequest {
    /// 用户凭据
    pub credentials: Credentials,
    /// 学期信息
    pub semester: Option<Semester>,
    /// provider配置
    pub provider_config: ProviderConfig,
}

/// 课程查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseResponse {
    /// 课程列表
    pub courses: Vec<Course>,
    /// 学期信息
    pub semester: Semester,
    /// 生成时间
    pub generated_at: DateTime<Utc>,
}

/// ICS生成选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IcsOptions {
    /// 日历名称
    pub calendar_name: Option<String>,
    /// 时区
    pub timezone: Option<String>,
    /// 是否包含课程描述
    pub include_description: bool,
    /// 是否包含教师信息
    pub include_teacher: bool,
    pub reminder_minutes: Option<u32>,
}

impl Default for IcsOptions {
    fn default() -> Self {
        Self {
            calendar_name: Some("CQUPT课程表".to_string()),
            timezone: Some("Asia/Shanghai".to_string()),
            include_description: true,
            include_teacher: true,
            reminder_minutes: Some(15),
        }
    }
}

/// 位置映射项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationMapping {
    /// 原始位置名称
    pub original: String,
    /// 标准化位置名称
    pub normalized: String,
    /// 建筑物
    pub building: Option<String>,
    /// 房间号
    pub room: Option<String>,
    /// 校区
    pub campus: Option<String>,
}
