use std::collections::HashMap;

use chrono::{DateTime, Utc};
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Semester {
    /// 学年
    pub year: u32,
    pub term: u32,
    /// 学期开始日期
    pub start_date: DateTime<Utc>,
    /// 学期结束日期
    pub end_date: DateTime<Utc>,
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
    pub semester: Semester,
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
