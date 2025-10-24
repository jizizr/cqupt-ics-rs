use crate::{
    Course, CourseRequest, CourseResponse, Result,
    providers::{
        BaseProvider, BaseProviderBuilder, ParamContext, ParamContextExt, Provider, ProviderInfo,
    },
};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveTime, TimeZone, Utc};
use reqwest::{StatusCode, Url, header};
use rsa::{Pkcs1v15Encrypt, RsaPublicKey, pkcs8::DecodePublicKey as _, rand_core::OsRng};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::{BTreeSet, HashMap, hash_map::Entry},
    hash::{self, Hash},
    ops::{Deref, DerefMut},
};

const API_ROOT: &str = "https://we.cqupt.edu.cn/";
const SCHEDULE_TYPES: &str = "[1,3,4]";
const SCHEDULE_FETCH_WEEKS: i64 = 25;
const PUBLIC_KEY: &str = concat!(
    "-----BEGIN PUBLIC KEY-----\n",
    "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAr9lk2DkxZdoK4KqKNJRW\n",
    "cIypatbmU+Ou/XvuuFHEK5AJ6e9zaICoo0RwHeBLFPoHdIBUeric+KP51i5FUWOz\n",
    "3EUfZY0Ogaey7sQHzx1rc3IKXy0pIwM3RASpkVmX70FMxa9wUvXNMtDlxurUbb5w\n",
    "XJ5wGPZs4tAwo9G+AbU1HfLwfRjrweEs0NpmlodHVeqrBqGQlBjJCUpqenwzJ+WD\n",
    "ds1FyFjGZmScAulPbChQ7Zlxhy6D1KC01O9LvycZNowZ7ovQ4i5V6b31lG9LNhKz\n",
    "qjJxbxElxApmpsNh3RSlE72GTHhMU8Y9J7Nc/Tt+an5HKlOU6LsB1PMeyoRbj/SN\n",
    "BQIDAQAB\n",
    "-----END PUBLIC KEY-----\n",
);
#[derive(Debug, Clone)]
struct WecquptTimeInfo {
    _term: String,
    start_date: DateTime<FixedOffset>,
    current_week: u32,
}

#[derive(Debug, Clone, Default)]
pub struct WecquptContext {
    time: Option<WecquptTimeInfo>,
    schedule: Option<WecquptScheduleResponse>,
}

#[derive(Debug, Clone, Deserialize)]
struct WecquptTimeResponse {
    code: i32,
    msg: Option<String>,
    data: WecquptTimeResponseData,
}

#[derive(Debug, Clone, Deserialize)]
struct WecquptTimeResponseData {
    time: WecquptTimePayload,
}

#[derive(Debug, Clone, Deserialize)]
struct WecquptTimePayload {
    term: String,
    #[serde(deserialize_with = "de_naive_date")]
    start_date: NaiveDate,
    week_num: u32,
    #[allow(dead_code)]
    weekday: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct WecquptScheduleResponse {
    code: i32,
    msg: Option<String>,
    data: WecquptScheduleData,
}

#[derive(Debug, Clone, Deserialize)]
struct WecquptScheduleData {
    #[serde(default)]
    schedules: Vec<WecquptScheduleItem>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Default)]
struct WecquptScheduleItemData {
    course_id: Option<String>,
    course_name: Option<String>,
    class_id: Option<String>,
    class_name: Option<String>,
    teacher_name: Option<String>,
    course_type: Option<String>,
    exam_type: Option<String>,
    seat: Option<String>,
    qualification: Option<String>,
    schedule_id: Option<String>,
    lecturer: Option<String>,
    chief_invigilator: Option<String>,
    deputy_invigilators: Option<Vec<String>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct WecquptScheduleItem {
    id: String,
    #[serde(rename = "type")]
    item_type: u32,
    #[serde(rename = "type_id")]
    type_id: Option<String>,
    #[serde(deserialize_with = "de_naive_date")]
    date: NaiveDate,
    #[serde(default, rename = "week_num")]
    week_num: Option<u32>,
    #[serde(rename = "start_time")]
    start_time: String,
    #[serde(rename = "end_time")]
    end_time: String,
    #[serde(default, rename = "time_slots")]
    time_slots: Vec<u32>,
    title: String,
    location: Option<String>,
    description: Option<String>,
    data: Option<WecquptScheduleItemData>,
}

fn de_naive_date<'de, D>(de: D) -> std::result::Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(de)?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(serde::de::Error::custom)
}

impl PartialEq for WecquptScheduleItem {
    fn eq(&self, other: &Self) -> bool {
        if !(self.type_id == other.type_id
            && self.start_time == other.start_time
            && self.end_time == other.end_time)
        {
            return false;
        }
        self.date.weekday() == other.date.weekday()
    }
}

impl Eq for WecquptScheduleItem {}

impl Hash for WecquptScheduleItem {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.start_time.hash(state);
        self.end_time.hash(state);
        self.date.weekday().hash(state);
    }
}

#[derive(Debug, Clone)]
struct ScheduleAccumulator {
    weeks: BTreeSet<u32>,
    earliest_date: NaiveDate,
}

impl ScheduleAccumulator {
    fn new(weeks: BTreeSet<u32>, earliest_date: NaiveDate) -> Self {
        Self {
            weeks,
            earliest_date,
        }
    }

    fn merge(&mut self, week: Option<u32>, date: NaiveDate) {
        if let Some(week) = week {
            self.weeks.insert(week);
        }
        if date < self.earliest_date {
            self.earliest_date = date;
        }
    }
}

struct WecquptSchedules {
    inner: HashMap<WecquptScheduleItem, ScheduleAccumulator>,
}

impl Deref for WecquptSchedules {
    type Target = HashMap<WecquptScheduleItem, ScheduleAccumulator>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for WecquptSchedules {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl WecquptSchedules {
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    fn insert(&mut self, item: WecquptScheduleItem) {
        let date = item.date;
        let week = item.week_num;
        match self.inner.entry(item) {
            Entry::Vacant(v) => {
                let mut weeks = BTreeSet::new();
                if let Some(week) = week {
                    weeks.insert(week);
                }
                v.insert(ScheduleAccumulator::new(weeks, date));
            }
            Entry::Occupied(mut o) => {
                o.get_mut().merge(week, date);
            }
        }
    }

    fn into_iter(self) -> impl Iterator<Item = (WecquptScheduleItem, ScheduleAccumulator)> {
        self.inner.into_iter()
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WecquptToken {
    pub x_token: String,
    pub refresh_token: String,
}

pub struct WecquptProvider {
    base: BaseProvider,
    base_url: Url,
    public_key: RsaPublicKey,
}

#[derive(Serialize)]
struct LoginForm<'a> {
    cqupt_id: &'a str,
    password: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification_code: Option<&'a str>,
}

impl WecquptProvider {
    pub fn new() -> Self {
        let builder = BaseProviderBuilder::new(ProviderInfo {
            name: "wecqupt".to_string(),
            description: "WE重邮 API".to_string(),
        });

        Self {
            base: builder.build(),
            base_url: Url::parse(API_ROOT).unwrap().join("api/").unwrap(),
            public_key: RsaPublicKey::from_public_key_pem(PUBLIC_KEY).unwrap(),
        }
    }

    fn ensure_context<'a>(
        &'a self,
        context: ParamContext<'a, WecquptContext>,
    ) -> Result<&'a mut WecquptContext> {
        let ctx = context.ensure_valid()?;
        if ctx.as_ref().is_none() {
            ctx.set(WecquptContext::default());
        }
        ctx.as_mut()
            .ok_or_else(|| self.base.custom_error("Failed to access provider context"))
    }

    fn encrypt_password(&self, password: &str) -> Result<String> {
        let mut rng = OsRng;
        let encrypted = self
            .public_key
            .encrypt(&mut rng, Pkcs1v15Encrypt, password.as_bytes())?;
        Ok(BASE64_STANDARD.encode(encrypted))
    }

    async fn fetch_time_info(&self, token: &WecquptToken) -> Result<WecquptTimeInfo> {
        if token.x_token.trim().is_empty() {
            return Err(self
                .base
                .custom_error("X-Token is required for wecqupt provider"));
        }
        let response = self
            .base
            .client
            .get(self.base_url.join("time").unwrap())
            .header("traefik", "jwzx")
            .header(header::COOKIE, &token.x_token)
            .send()
            .await
            .map_err(|e| self.base.handle_error_req(e))?;

        if !response.status().is_success() {
            return Err(self
                .base
                .custom_error(format!("HTTP {} error", response.status())));
        }

        let payload: WecquptTimeResponse = response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse time response: {}", e))
        })?;

        if payload.code != 0 {
            return Err(self.base.custom_error(
                payload
                    .msg
                    .unwrap_or_else(|| "Failed to fetch time info".to_string()),
            ));
        }

        let naive = payload
            .data
            .time
            .start_date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| self.base.custom_error("Invalid start date time"))?;
        let tz = self.timezone();
        let start_date = tz
            .from_local_datetime(&naive)
            .single()
            .ok_or_else(|| self.base.custom_error("Failed to convert start date"))?;

        Ok(WecquptTimeInfo {
            _term: payload.data.time.term,
            start_date,
            current_week: payload.data.time.week_num,
        })
    }

    async fn fetch_schedule(
        &self,
        semester_start: &DateTime<FixedOffset>,
        token: &WecquptToken,
    ) -> Result<WecquptScheduleResponse> {
        let start_str = semester_start.format("%Y-%m-%d").to_string();
        let end_date = *semester_start + chrono::Duration::weeks(SCHEDULE_FETCH_WEEKS)
            - chrono::Duration::days(1);
        let end_str = end_date.format("%Y-%m-%d").to_string();

        let response = self
            .base
            .client
            .get(self.base_url.join("timetable").unwrap())
            .header("traefik", "jwzx")
            .header(header::COOKIE, &token.x_token)
            .query(&[
                ("start_date", start_str.as_str()),
                ("end_date", end_str.as_str()),
                ("types", SCHEDULE_TYPES),
            ])
            .send()
            .await
            .map_err(|e| self.base.handle_error_req(e))?;

        if response.status() != StatusCode::FORBIDDEN
            && response.url().path() == "/rump_frontend/access_forbidden/"
        {
            return Err(crate::Error::CurfewTime(()));
        }

        if !response.status().is_success() {
            return Err(self
                .base
                .custom_error(format!("HTTP {} error", response.status())));
        }

        let payload: WecquptScheduleResponse = response.json().await.map_err(|e| {
            self.base
                .custom_error(format!("Failed to parse schedule response: {}", e))
        })?;

        if payload.code != 0 {
            return Err(self.base.custom_error(
                payload
                    .msg
                    .unwrap_or_else(|| "Failed to fetch schedule".to_string()),
            ));
        }

        Ok(payload)
    }

    fn aggregate_schedule_items(
        &self,
        items: Vec<WecquptScheduleItem>,
    ) -> Result<WecquptSchedules> {
        let mut wss: WecquptSchedules = WecquptSchedules::new();

        for item in items {
            wss.insert(item);
        }

        Ok(wss)
    }

    fn convert_schedule_to_courses(
        &self,
        items: Vec<WecquptScheduleItem>,
        time_info: &WecquptTimeInfo,
    ) -> Result<Vec<Course>> {
        let aggregated = self.aggregate_schedule_items(items)?;
        let courses = aggregated
            .into_iter()
            .map(|(item, acc)| self.build_course(item, acc, time_info))
            .collect::<Result<Vec<_>>>()?;
        Ok(courses)
    }

    fn build_course(
        &self,
        item: WecquptScheduleItem,
        acc: ScheduleAccumulator,
        time_info: &WecquptTimeInfo,
    ) -> Result<Course> {
        let data = item.data.unwrap_or_default();
        let start_time = self.combine_datetime(acc.earliest_date, &item.start_time)?;
        let end_time = self.combine_datetime(acc.earliest_date, &item.end_time)?;
        let weeks = acc.weeks.into_iter().collect::<Vec<_>>();
        let teacher = Self::normalize_ref(data.teacher_name.as_ref());
        let code = Self::normalize_ref(data.course_id.as_ref())
            .or_else(|| Self::normalize_ref(item.type_id.as_ref()))
            .or_else(|| Self::normalize_ref(data.class_id.as_ref()));
        let location = Self::normalize_ref(item.location.as_ref());
        let description = Self::normalize_ref(item.description.as_ref());
        let exam_type = Self::normalize_ref(data.exam_type.as_ref());
        let seat = Self::normalize_ref(data.seat.as_ref());
        let status = Self::normalize_ref(data.qualification.as_ref());

        let course_type = match item.item_type {
            1 => Self::normalize_ref(data.course_type.as_ref()),
            3 => Some("考试".to_string()),
            4 => Some("自定义日程".to_string()),
            _ => None,
        };

        let mut begin_lesson = None;
        if !item.time_slots.is_empty() {
            begin_lesson = item.time_slots.iter().copied().min();
        }

        let lesson_duration = if item.time_slots.is_empty() {
            None
        } else {
            Some(item.time_slots.len() as u32)
        };

        let weekday = acc.earliest_date.weekday().number_from_monday();

        Ok(Course {
            name: item.title,
            code,
            teacher,
            location,
            start_time,
            end_time,
            note: description,
            course_type,
            weeks: Some(weeks),
            weekday: Some(weekday),
            begin_lesson,
            lesson_duration,
            current_week: Some(time_info.current_week),
            exam_type,
            seat,
            status,

            ..Default::default()
        })
    }

    fn parse_time(&self, time_str: &str) -> Result<NaiveTime> {
        NaiveTime::parse_from_str(time_str, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(time_str, "%H:%M"))
            .map_err(|_| {
                self.base
                    .custom_error(format!("Invalid time format: {}", time_str))
            })
    }

    fn combine_datetime(&self, date: NaiveDate, time_str: &str) -> Result<DateTime<FixedOffset>> {
        let time = self.parse_time(time_str)?;
        let naive = date.and_time(time);
        self.timezone()
            .from_local_datetime(&naive)
            .single()
            .ok_or_else(|| self.base.custom_error("Failed to create datetime"))
    }

    fn normalize_ref(value: Option<&String>) -> Option<String> {
        value.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }
}

impl Default for WecquptProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for WecquptProvider {
    type Token = WecquptToken;
    type ContextType = WecquptContext;

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
        let mut token = Self::Token::default();

        let response = self
            .base
            .client
            .post(self.base_url.join("login").unwrap())
            .header("traefik", "user")
            .form(&LoginForm {
                cqupt_id: &request.credentials.username,
                password: &self.encrypt_password(&request.credentials.password)?,
                verification_code: None,
            })
            .send()
            .await?;
        if response.status() == StatusCode::FORBIDDEN
            && response.url().path() == "/rump_frontend/access_forbidden/"
        {
            return Err(crate::Error::CurfewTime(()));
        }

        if !response.status().is_success() {
            return Err(self
                .base
                .custom_error(format!("HTTP {} error", response.status())));
        }

        for ck in response.headers().get_all(header::SET_COOKIE) {
            let ck = ck.to_str().map_err(|e| {
                self.base
                    .custom_error(format!("Failed to parse Set-Cookie header: {}", e))
            })?;
            if ck.starts_with("x-token") {
                token.x_token = ck.to_string();
            } else if ck.starts_with("refresh-token") {
                token.refresh_token = ck.to_string();
            }
        }
        if token.x_token.is_empty() || token.refresh_token.is_empty() {
            Err(self
                .base
                .custom_error("Failed to retrieve authentication tokens"))
        } else {
            Ok(token)
        }
    }

    async fn validate_token(&self, token: &Self::Token) -> Result<bool> {
        Ok(!token.x_token.trim().is_empty())
    }

    async fn get_semester_start<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        _request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<DateTime<FixedOffset>> {
        let ctx = self.ensure_context(context)?;
        if ctx.time.is_none() {
            let info = self.fetch_time_info(token).await?;
            ctx.time = Some(info);
        }

        Ok(ctx
            .time
            .as_ref()
            .map(|info| info.start_date)
            .ok_or_else(|| self.base.custom_error("Failed to load semester start"))?)
    }

    async fn get_courses<'a, 'b>(
        &'a self,
        context: ParamContext<'b, Self::ContextType>,
        request: &mut CourseRequest,
        token: &Self::Token,
    ) -> Result<CourseResponse> {
        let ctx = self.ensure_context(context)?;
        if ctx.time.is_none() {
            let info = self.fetch_time_info(token).await?;
            ctx.time = Some(info);
        }
        let time_info = ctx
            .time
            .clone()
            .ok_or_else(|| self.base.custom_error("Missing time info"))?;

        let semester = request
            .semester
            .as_ref()
            .ok_or_else(|| self.base.custom_error("Semester start date is required"))?;

        if ctx.schedule.is_none() {
            let schedule = self.fetch_schedule(&semester.start_date, token).await?;
            ctx.schedule = Some(schedule);
        }

        let schedule = ctx
            .schedule
            .clone()
            .ok_or_else(|| self.base.custom_error("Failed to load schedule"))?;

        let courses = self.convert_schedule_to_courses(schedule.data.schedules, &time_info)?;

        Ok(CourseResponse {
            courses,
            semester: semester.clone(),
            generated_at: Utc::now().with_timezone(&self.timezone()),
        })
    }

    async fn refresh_token(&self, _token: &Self::Token) -> Result<Self::Token> {
        Err(self
            .base
            .custom_error("Token refresh is not supported for wecqupt provider"))
    }

    fn token_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(3600 * 24 * 20)
    }
}
