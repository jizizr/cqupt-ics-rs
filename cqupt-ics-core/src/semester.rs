use chrono::{DateTime, Datelike, Local, TimeZone, Utc};

use crate::types::Semester;

/// 学期类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemesterType {
    /// 秋季学期（第1学期）：8月-次年1月
    Autumn = 1,
    /// 春季学期（第2学期）：2月-8月  
    Spring = 2,
}

impl SemesterType {
    /// 获取学期的开始和结束月份
    pub fn date_range(self, academic_year: u32) -> (DateTime<Utc>, DateTime<Utc>) {
        let utc = Utc;
        match self {
            SemesterType::Autumn => (
                // 9月1日
                utc.with_ymd_and_hms(academic_year as i32, 9, 1, 0, 0, 0)
                    .unwrap(),
                // 次年1月31日
                utc.with_ymd_and_hms(academic_year as i32 + 1, 1, 31, 23, 59, 59)
                    .unwrap(),
            ),
            SemesterType::Spring => (
                // 2月15日（春节后）
                utc.with_ymd_and_hms(academic_year as i32 + 1, 2, 15, 0, 0, 0)
                    .unwrap(),
                // 6月30日
                utc.with_ymd_and_hms(academic_year as i32 + 1, 6, 30, 23, 59, 59)
                    .unwrap(),
            ),
        }
    }
}

/// 学期判断器
pub struct SemesterDetector;

impl SemesterDetector {
    /// 根据当前时间自动判断当前学期
    ///
    /// 返回 (学年, 学期号, 学期类型)
    /// 例如：(2024, 1, SemesterType::Autumn) 表示2024-2025学年第1学期
    pub fn detect_current() -> (u32, u32, SemesterType) {
        let now = Local::now();
        Self::detect_from_date(now.with_timezone(&Utc))
    }

    /// 根据指定日期判断学期
    pub fn detect_from_date(date: DateTime<Utc>) -> (u32, u32, SemesterType) {
        let year = date.year() as u32;
        let month = date.month();

        match month {
            // 1月：属于上一学年的秋季学期
            1 => (year - 1, 1, SemesterType::Autumn),
            // 2-8月：当前学年的春季学期（包含暑假）
            2..=8 => (year - 1, 2, SemesterType::Spring),
            // 9-12月：当前学年的秋季学期
            9..=12 => (year, 1, SemesterType::Autumn),
            _ => unreachable!(),
        }
    }

    /// 创建带有准确日期范围的学期对象
    pub fn create_semester(academic_year: u32, term: u32) -> Result<Semester, String> {
        let semester_type = match term {
            1 => SemesterType::Autumn,
            2 => SemesterType::Spring,
            _ => {
                return Err(format!(
                    "无效的学期号: {}，只支持1（秋季）和2（春季）",
                    term
                ));
            }
        };

        let (start_date, end_date) = semester_type.date_range(academic_year);

        Ok(Semester {
            year: academic_year,
            term,
            start_date,
            end_date,
        })
    }

    /// 创建当前学期对象
    pub fn create_current_semester() -> Semester {
        let (year, term, semester_type) = Self::detect_current();
        let (start_date, end_date) = semester_type.date_range(year);

        Semester {
            year,
            term,
            start_date,
            end_date,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_semester_detection() {
        // 测试2024年9月（秋季学期开始）
        let date = Utc.with_ymd_and_hms(2024, 9, 15, 12, 0, 0).unwrap();
        let (year, term, semester_type) = SemesterDetector::detect_from_date(date);
        assert_eq!(year, 2024);
        assert_eq!(term, 1);
        assert_eq!(semester_type, SemesterType::Autumn);

        // 测试2025年1月（秋季学期结束）
        let date = Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0).unwrap();
        let (year, term, semester_type) = SemesterDetector::detect_from_date(date);
        assert_eq!(year, 2024);
        assert_eq!(term, 1);
        assert_eq!(semester_type, SemesterType::Autumn);

        // 测试2025年3月（春季学期）
        let date = Utc.with_ymd_and_hms(2025, 3, 15, 12, 0, 0).unwrap();
        let (year, term, semester_type) = SemesterDetector::detect_from_date(date);
        assert_eq!(year, 2024);
        assert_eq!(term, 2);
        assert_eq!(semester_type, SemesterType::Spring);

        // 测试2025年7月（归入春季学期）
        let date = Utc.with_ymd_and_hms(2025, 7, 15, 12, 0, 0).unwrap();
        let (year, term, semester_type) = SemesterDetector::detect_from_date(date);
        assert_eq!(year, 2024);
        assert_eq!(term, 2);
        assert_eq!(semester_type, SemesterType::Spring);
    }

    #[test]
    fn test_semester_creation() {
        let semester = SemesterDetector::create_semester(2024, 1).unwrap();
        assert_eq!(semester.year, 2024);
        assert_eq!(semester.term, 1);

        // 验证日期范围
        assert_eq!(semester.start_date.month(), 9);
        assert_eq!(semester.start_date.year(), 2024);
        assert_eq!(semester.end_date.month(), 1);
        assert_eq!(semester.end_date.year(), 2025);
    }

    #[test]
    fn test_invalid_term() {
        let result = SemesterDetector::create_semester(2024, 3);
        assert!(result.is_err());

        let result = SemesterDetector::create_semester(2024, 4);
        assert!(result.is_err());
    }
}
