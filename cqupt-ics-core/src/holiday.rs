use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime};
use ical::parser::ical::{IcalParser, component::IcalEvent};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use crate::{Course, CourseResponse, Error, Result, Semester};

/// 节假日调休信息
#[derive(Debug, Clone)]
pub struct HolidayCalendar {
    rest_days: BTreeSet<NaiveDate>,
    rest_to_makeup: HashMap<NaiveDate, NaiveDate>,
    makeup_days: BTreeSet<NaiveDate>,
}

impl HolidayCalendar {
    /// 从文件路径加载节假日ICS
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let file = File::open(path_ref).map_err(|err| {
            Error::Config(format!(
                "无法打开节假日ICS文件 {}: {}",
                path_ref.display(),
                err
            ))
        })?;
        Self::from_reader(file)
    }

    /// 从字节切片加载节假日ICS
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self> {
        let cursor = std::io::Cursor::new(bytes.as_ref());
        Self::from_reader(cursor)
    }

    /// 从读取器中加载节假日ICS
    pub fn from_reader<R: Read>(reader: R) -> Result<Self> {
        let parser = IcalParser::new(BufReader::new(reader));
        let mut groups: BTreeMap<String, HolidayGroup> = BTreeMap::new();

        for calendar in parser {
            let calendar =
                calendar.map_err(|err| Error::Config(format!("节假日ICS解析失败: {}", err)))?;

            for event in calendar.events {
                let Some(kind) = classify_event(&event) else {
                    continue;
                };

                let dates = extract_event_dates(&event)?;
                if dates.is_empty() {
                    continue;
                }

                let key = event_property_owned(&event, "X-APPLE-UNIVERSAL-ID")
                    .or_else(|| event_property_owned(&event, "UID"))
                    .or_else(|| event_property_owned(&event, "SUMMARY"))
                    .unwrap_or_else(|| format!("{:?}-{:?}", kind, dates));

                let entry = groups.entry(key).or_default();
                match kind {
                    HolidayEventKind::Rest => entry.rest.extend(dates),
                    HolidayEventKind::Makeup => entry.makeup.extend(dates),
                }
            }
        }

        Self::build(groups)
    }

    /// 将节假日调整应用到课程响应
    pub fn apply_to_response(&self, response: &mut CourseResponse) {
        self.apply_to_courses(&mut response.courses, &response.semester);
    }

    /// 是否为放假日
    pub fn is_rest_day(&self, date: NaiveDate) -> bool {
        self.rest_days.contains(&date)
    }

    /// 是否为调休上班日
    pub fn is_makeup_day(&self, date: NaiveDate) -> bool {
        self.makeup_days.contains(&date)
    }

    /// 获取放假日对应的调休补课日
    pub fn makeup_for(&self, rest_date: NaiveDate) -> Option<NaiveDate> {
        self.rest_to_makeup.get(&rest_date).copied()
    }

    /// 获取调休补课日对应的放假日
    pub fn rest_for_makeup(&self, makeup_date: NaiveDate) -> Option<NaiveDate> {
        self.rest_to_makeup.iter().find_map(|(&rest, &makeup)| {
            if makeup == makeup_date {
                Some(rest)
            } else {
                None
            }
        })
    }

    /// 将节假日调整应用到课程列表
    pub fn apply_to_courses(&self, courses: &mut Vec<Course>, semester: &Semester) {
        if courses.is_empty() {
            return;
        }
        let len = courses.len();
        for i in 0..len {
            let (Some(weeks), Some(weekday)) = (courses[i].weeks.take(), courses[i].weekday) else {
                handle_single_occurrence_course(&self.rest_to_makeup, courses, i);
                continue;
            };

            if weeks.is_empty() {
                handle_single_occurrence_course(&self.rest_to_makeup, courses, i);
                continue;
            }

            let original_first_week = weeks.first().copied().unwrap();
            let original_start = courses[i].start_time;
            let original_end: DateTime<FixedOffset> = courses[i].end_time;
            let mut off_weeks = vec![];
            for week in weeks.iter().copied() {
                let occurrence_date: NaiveDate = occurrence_date_for(semester, week, weekday);
                if self.rest_days.contains(&occurrence_date) {
                    if let Some(makeup_date) = self.rest_to_makeup.get(&occurrence_date) {
                        let occurrence_start =
                            shift_weeks(original_start, week, original_first_week);
                        let occurrence_end = shift_weeks(original_end, week, original_first_week);
                        let makeup_course = create_makeup_course(
                            &courses[i],
                            occurrence_start,
                            occurrence_end,
                            occurrence_date,
                            *makeup_date,
                        );
                        courses.push(makeup_course);
                    }
                    off_weeks.push(week);
                }
            }
            let course = &mut courses[i];
            course.weeks = Some(weeks);
            course.off_weeks = if off_weeks.is_empty() {
                None
            } else {
                Some(off_weeks)
            };
        }
    }

    fn build(groups: BTreeMap<String, HolidayGroup>) -> Result<Self> {
        const CLUSTER_GAP_DAYS: i64 = 45;
        const MAKEUP_WINDOW_DAYS: i64 = 21;

        let mut rest_days = BTreeSet::new();
        let mut rest_to_makeup = HashMap::new();
        let mut makeup_days = BTreeSet::new();

        for group in groups.values() {
            if group.rest.is_empty() {
                continue;
            }

            rest_days.extend(&group.rest);
            makeup_days.extend(&group.makeup);

            if group.makeup.is_empty() {
                continue;
            }

            let clusters = cluster_dates(&group.rest, CLUSTER_GAP_DAYS);
            let mut available_makeups = group.makeup.clone();

            for cluster in clusters {
                if cluster.is_empty() {
                    continue;
                }

                let first = *cluster.first().unwrap();
                let last = *cluster.last().unwrap();
                let window_start = first
                    .checked_sub_signed(Duration::days(MAKEUP_WINDOW_DAYS))
                    .unwrap_or(first);
                let window_end = last
                    .checked_add_signed(Duration::days(MAKEUP_WINDOW_DAYS))
                    .unwrap_or(last);

                let candidate_dates: Vec<NaiveDate> = available_makeups
                    .range(window_start..=window_end)
                    .cloned()
                    .collect();

                for date in &candidate_dates {
                    available_makeups.remove(date);
                }

                if candidate_dates.is_empty() {
                    continue;
                }

                let mut before = Vec::new();
                let mut within = Vec::new();
                let mut after = Vec::new();

                for date in candidate_dates {
                    if date < first {
                        before.push(date);
                    } else if date > last {
                        after.push(date);
                    } else {
                        within.push(date);
                    }
                }

                before.sort_by(|a, b| b.cmp(a));
                after.sort();
                let rest_sorted = cluster;

                within.sort_by_key(|date| {
                    rest_sorted
                        .iter()
                        .map(|rest| (rest.signed_duration_since(*date).num_days()).abs())
                        .min()
                        .unwrap_or(0)
                });

                let mut assigned: Vec<Option<NaiveDate>> = vec![None; rest_sorted.len()];
                let mut preferred_indices: Vec<usize> = rest_sorted
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, date)| if is_workday(*date) { Some(idx) } else { None })
                    .collect();
                if preferred_indices.is_empty() {
                    preferred_indices = (0..rest_sorted.len()).collect();
                }
                preferred_indices.sort();

                let mut fallback_indices: Vec<usize> = (0..rest_sorted.len()).collect();
                fallback_indices.sort();

                let mut assign = |makeup_date: NaiveDate| {
                    assign_makeup(
                        &mut assigned,
                        &mut preferred_indices,
                        &mut fallback_indices,
                        makeup_date,
                    );
                };

                for date in after {
                    assign(date);
                }
                for date in before {
                    assign(date);
                }
                for date in within {
                    assign(date);
                }

                for (idx, maybe_makeup) in assigned.into_iter().enumerate() {
                    if let Some(makeup_date) = maybe_makeup {
                        let rest_date = rest_sorted[idx];
                        rest_to_makeup.insert(rest_date, makeup_date);
                    }
                }
            }
        }

        Ok(Self {
            rest_days,
            rest_to_makeup,
            makeup_days,
        })
    }
}

#[derive(Default)]
struct HolidayGroup {
    rest: BTreeSet<NaiveDate>,
    makeup: BTreeSet<NaiveDate>,
}

#[derive(Debug, Clone, Copy)]
enum HolidayEventKind {
    Rest,
    Makeup,
}

fn classify_event(event: &IcalEvent) -> Option<HolidayEventKind> {
    if let Some(kind) = event_property(event, "X-APPLE-SPECIAL-DAY") {
        return match kind {
            "WORK-HOLIDAY" => Some(HolidayEventKind::Rest),
            "ALTERNATE-WORKDAY" => Some(HolidayEventKind::Makeup),
            _ => None,
        };
    }

    let summary = event_property(event, "SUMMARY")?;
    let normalized = summary.replace([' ', '\t'], "");
    if normalized.contains('休') || normalized.contains("放假") {
        return Some(HolidayEventKind::Rest);
    }
    if normalized.contains('班') || normalized.contains("调休") || normalized.contains("上班")
    {
        return Some(HolidayEventKind::Makeup);
    }

    None
}

fn extract_event_dates(event: &IcalEvent) -> Result<Vec<NaiveDate>> {
    let start_raw = event_property(event, "DTSTART")
        .ok_or_else(|| Error::Config("节假日ICS事件缺少DTSTART字段".to_string()))?;

    let start = parse_date(start_raw)
        .map_err(|err| Error::Config(format!("无法解析节假日开始日期 {}: {}", start_raw, err)))?;

    let exclusive_end = match event_property(event, "DTEND") {
        Some(value) => parse_date(value)
            .map_err(|err| Error::Config(format!("无法解析节假日结束日期 {}: {}", value, err)))?,
        None => start
            .checked_add_signed(Duration::days(1))
            .ok_or_else(|| Error::Config("节假日日期范围过大".to_string()))?,
    };

    if exclusive_end <= start {
        return Ok(vec![start]);
    }

    let mut dates = Vec::new();
    let mut current = start;
    while current < exclusive_end {
        dates.push(current);
        current = current
            .checked_add_signed(Duration::days(1))
            .ok_or_else(|| Error::Config("节假日日期计算溢出".to_string()))?;
    }

    if dates.is_empty() {
        dates.push(start);
    }

    Ok(dates)
}

fn event_property<'a>(event: &'a IcalEvent, name: &str) -> Option<&'a str> {
    event
        .properties
        .iter()
        .find(|prop| prop.name.eq_ignore_ascii_case(name))
        .and_then(|prop| prop.value.as_deref())
}

fn event_property_owned(event: &IcalEvent, name: &str) -> Option<String> {
    event_property(event, name).map(|value| value.to_string())
}

fn parse_date(value: &str) -> std::result::Result<NaiveDate, chrono::ParseError> {
    NaiveDate::parse_from_str(value, "%Y%m%d")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").map(|dt| dt.date()))
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ").map(|dt| dt.date()))
        .or_else(|_| DateTime::parse_from_rfc3339(value).map(|dt| dt.date_naive()))
}

fn occurrence_date_for(semester: &Semester, week: u32, weekday: u32) -> NaiveDate {
    let week_start = semester
        .start_date
        .checked_add_signed(Duration::weeks((week.saturating_sub(1)) as i64))
        .unwrap_or(semester.start_date);
    let date = week_start
        .checked_add_signed(Duration::days((weekday.saturating_sub(1)) as i64))
        .unwrap_or(week_start);
    date.date_naive()
}

fn shift_weeks(
    base: DateTime<FixedOffset>,
    target_week: u32,
    base_week: u32,
) -> DateTime<FixedOffset> {
    let diff = target_week as i64 - base_week as i64;
    base + Duration::weeks(diff)
}

fn create_makeup_course(
    template: &Course,
    occurrence_start: DateTime<FixedOffset>,
    occurrence_end: DateTime<FixedOffset>,
    rest_date: NaiveDate,
    makeup_date: NaiveDate,
) -> Course {
    let diff_days = makeup_date.signed_duration_since(rest_date).num_days();
    let delta = Duration::days(diff_days);

    let mut course = template.clone();
    course.start_time = occurrence_start + delta;
    course.end_time = occurrence_end + delta;
    course.weeks = None;
    course.weekday = None;
    course.current_week = None;

    let rest_fmt = rest_date.format("%Y-%m-%d");
    let makeup_fmt = makeup_date.format("%Y-%m-%d");

    let note = format!("调休补课：原日期 {}", rest_fmt);
    course.note = match course.note {
        Some(ref desc) if !desc.is_empty() => Some(format!("{desc}\n{note}")),
        _ => Some(note),
    };
    course.raw_week = Some(format!("调休补课（{} → {}）", rest_fmt, makeup_fmt));

    course
}

fn handle_single_occurrence_course(
    rest_to_makeup: &HashMap<NaiveDate, NaiveDate>,
    courses: &mut Vec<Course>,
    index: usize,
) {
    let course = &courses[index];
    let date = course.start_time.date_naive();
    if let Some(makeup_date) = rest_to_makeup.get(&date) {
        let diff_days = makeup_date.signed_duration_since(date).num_days();
        let delta = Duration::days(diff_days);
        let mut moved = course.clone();
        moved.start_time = course.start_time + delta;
        moved.end_time = course.end_time + delta;
        moved.note = match moved.note {
            Some(ref desc) if !desc.is_empty() => Some(format!(
                "{desc}\n调休补课：原日期 {}",
                date.format("%Y-%m-%d")
            )),
            _ => Some(format!("调休补课：原日期 {}", date.format("%Y-%m-%d"))),
        };
        moved.raw_week = Some(format!(
            "调休补课（{} → {}）",
            date.format("%Y-%m-%d"),
            makeup_date.format("%Y-%m-%d")
        ));
        courses.push(moved);
    }
}

fn cluster_dates(dates: &BTreeSet<NaiveDate>, max_gap_days: i64) -> Vec<Vec<NaiveDate>> {
    let mut clusters = Vec::new();
    let mut current = Vec::new();

    for date in dates {
        if let Some(prev) = current.last() {
            let gap = date.signed_duration_since(*prev).num_days();
            if gap > max_gap_days {
                clusters.push(current);
                current = Vec::new();
            }
        }
        current.push(*date);
    }

    if !current.is_empty() {
        clusters.push(current);
    }

    clusters
}

fn is_workday(date: NaiveDate) -> bool {
    matches!(
        date.weekday(),
        chrono::Weekday::Mon
            | chrono::Weekday::Tue
            | chrono::Weekday::Wed
            | chrono::Weekday::Thu
            | chrono::Weekday::Fri
    )
}

fn assign_makeup(
    assigned: &mut [Option<NaiveDate>],
    preferred_indices: &mut Vec<usize>,
    fallback_indices: &mut Vec<usize>,
    date: NaiveDate,
) {
    while let Some(idx) = preferred_indices.pop() {
        if assigned[idx].is_none() {
            assigned[idx] = Some(date);
            return;
        }
    }

    while let Some(idx) = fallback_indices.pop() {
        if assigned[idx].is_none() {
            assigned[idx] = Some(date);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn load_calendar() -> HolidayCalendar {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("下载.ics");
        HolidayCalendar::from_path(path).expect("failed to load test holiday calendar")
    }

    #[test]
    fn national_day_2025_makeup_days() {
        let calendar = load_calendar();
        let rest_oct7 = NaiveDate::from_ymd_opt(2025, 10, 7).unwrap();

        assert_eq!(
            calendar.makeup_for(rest_oct7),
            Some(NaiveDate::from_ymd_opt(2025, 9, 28).unwrap())
        );
    }

    #[test]
    fn spring_festival_2025_adjustments() {
        let calendar = load_calendar();
        let holiday_dates: Vec<_> = (0..8)
            .map(|offset| NaiveDate::from_ymd_opt(2025, 1, 28).unwrap() + Duration::days(offset))
            .collect();

        for date in &holiday_dates {
            assert!(
                calendar.is_rest_day(*date),
                "expected {} to be holiday (rest day)",
                date
            );
        }

        let rest_feb03 = NaiveDate::from_ymd_opt(2025, 2, 3).unwrap();
        let rest_feb04 = NaiveDate::from_ymd_opt(2025, 2, 4).unwrap();
        assert_eq!(
            calendar.makeup_for(rest_feb03),
            Some(NaiveDate::from_ymd_opt(2025, 1, 26).unwrap())
        );
        assert_eq!(
            calendar.makeup_for(rest_feb04),
            Some(NaiveDate::from_ymd_opt(2025, 2, 8).unwrap())
        );

        let makeup_jan26 = NaiveDate::from_ymd_opt(2025, 1, 26).unwrap();
        let makeup_feb08 = NaiveDate::from_ymd_opt(2025, 2, 8).unwrap();
        assert!(calendar.is_makeup_day(makeup_jan26));
        assert!(calendar.is_makeup_day(makeup_feb08));
        assert_eq!(calendar.rest_for_makeup(makeup_jan26), Some(rest_feb03));
        assert_eq!(calendar.rest_for_makeup(makeup_feb08), Some(rest_feb04));

        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let semester_start = tz.with_ymd_and_hms(2025, 1, 6, 0, 0, 0).unwrap();
        let semester = Semester {
            start_date: semester_start,
        };

        let mut response = CourseResponse {
            courses: vec![
                Course {
                    name: "软件工程导论".to_string(),
                    start_time: tz.with_ymd_and_hms(2025, 1, 6, 8, 0, 0).unwrap(),
                    end_time: tz.with_ymd_and_hms(2025, 1, 6, 10, 0, 0).unwrap(),
                    weeks: Some(vec![1, 2, 3, 4, 5, 6]),
                    weekday: Some(1),
                    ..Default::default()
                },
                Course {
                    name: "操作系统".to_string(),
                    start_time: tz.with_ymd_and_hms(2025, 1, 7, 14, 0, 0).unwrap(),
                    end_time: tz.with_ymd_and_hms(2025, 1, 7, 16, 0, 0).unwrap(),
                    weeks: Some(vec![1, 2, 3, 4, 5]),
                    weekday: Some(2),
                    ..Default::default()
                },
            ],
            semester: semester.clone(),
            generated_at: tz.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        };

        calendar.apply_to_response(&mut response);

        let monday_course = response
            .courses
            .iter()
            .find(|course| course.name == "软件工程导论" && course.off_weeks.is_some())
            .expect("monday course missing");
        assert!(monday_course.off_weeks.as_ref().unwrap().contains(&5));

        let makeup_monday = response
            .courses
            .iter()
            .find(|course| {
                course.name == "软件工程导论"
                    && course
                        .raw_week
                        .as_deref()
                        .is_some_and(|raw| raw.contains("调休补课"))
            })
            .expect("missing monday makeup course");
        assert_eq!(
            makeup_monday.start_time.date_naive(),
            NaiveDate::from_ymd_opt(2025, 1, 26).unwrap()
        );

        let tuesday_course = response
            .courses
            .iter()
            .find(|course| course.name == "操作系统" && course.off_weeks.is_some())
            .expect("tuesday course missing");
        assert!(tuesday_course.off_weeks.as_ref().unwrap().contains(&5));

        let makeup_tuesday = response
            .courses
            .iter()
            .find(|course| {
                course.name == "操作系统"
                    && course
                        .raw_week
                        .as_deref()
                        .is_some_and(|raw| raw.contains("调休补课"))
            })
            .expect("missing tuesday makeup course");
        assert_eq!(
            makeup_tuesday.start_time.date_naive(),
            NaiveDate::from_ymd_opt(2025, 2, 8).unwrap()
        );
    }
}
