use super::*;
use crate::{Course, CourseResponse, Semester};
use chrono::Utc;
use std::collections::HashMap;

#[test]
fn test_new_description_format() {
    let mut extra_class = HashMap::new();
    extra_class.insert("course_num".to_string(), "CS100001".to_string());
    extra_class.insert("raw_week".to_string(), "1,2,3,4,5,6,7,8".to_string());
    extra_class.insert("current_week".to_string(), "3".to_string());

    let class_course = Course {
        name: "计算机基础课程".to_string(),
        code: Some("CS100001".to_string()),
        teacher: Some("张老师".to_string()),
        location: Some("2108".to_string()),
        start_time: Utc::now(),
        end_time: Utc::now(),
        description: None,
        course_type: Some("必修".to_string()),
        credits: Some(2.0),
        recurrence: None,
        extra: extra_class,
    };

    let mut extra_exam = HashMap::new();
    extra_exam.insert("exam_type".to_string(), "期末".to_string());
    extra_exam.insert("status".to_string(), "正常".to_string());
    extra_exam.insert("seat".to_string(), "A001".to_string());
    extra_exam.insert("week".to_string(), "16".to_string());

    let exam_course = Course {
        name: "计算机基础课程".to_string(),
        code: Some("CS100001".to_string()),
        teacher: None,
        location: Some("2108".to_string()),
        start_time: Utc::now(),
        end_time: Utc::now(),
        description: None,
        course_type: Some("考试".to_string()),
        credits: None,
        recurrence: None,
        extra: extra_exam,
    };

    // 创建课程响应
    let response = CourseResponse {
        courses: vec![class_course, exam_course],
        semester: Semester {
            year: 2024,
            term: 1,
            start_date: Utc::now(),
            end_date: Utc::now(),
        },
        generated_at: Utc::now(),
    };

    // 生成ICS
    let options = IcsOptions::default();
    let generator = IcsGenerator::new(options);

    let ics_content = generator.generate(&response).expect("生成ICS失败");

    println!("✅ 成功生成ICS内容:");
    println!("---");
    // 只打印SUMMARY和DESCRIPTION部分
    for line in ics_content.lines() {
        if line.starts_with("SUMMARY:") || line.starts_with("DESCRIPTION:") {
            println!("{}", line);
        }
    }
    println!("---");

    // 验证内容包含期望的格式
    assert!(ics_content.contains("SUMMARY:计算机基础课程 - 2108"));
    assert!(ics_content.contains("SUMMARY:[期末考试] 计算机基础课程 - 2108"));
    assert!(ics_content.contains("任课教师: 张老师"));
    assert!(ics_content.contains("该课程是必修课"));
    assert!(ics_content.contains("考试座位号是A001"));
    assert!(ics_content.contains("考试状态: 正常"));
}

#[test]
fn test_location_with_geo() {
    // 测试新的地理位置功能
    let mut extra_class = HashMap::new();
    extra_class.insert("course_num".to_string(), "MATH20001".to_string());
    extra_class.insert("raw_week".to_string(), "1-16".to_string());

    let course = Course {
        name: "高等数学课程".to_string(),
        code: Some("MATH20001".to_string()),
        teacher: Some("李老师".to_string()),
        location: Some("4307".to_string()), // 四教
        start_time: Utc::now(),
        end_time: Utc::now(),
        description: None,
        course_type: Some("必修".to_string()),
        credits: Some(2.0),
        recurrence: None,
        extra: extra_class,
    };

    let response = CourseResponse {
        courses: vec![course],
        semester: Semester {
            year: 2024,
            term: 1,
            start_date: Utc::now(),
            end_date: Utc::now(),
        },
        generated_at: Utc::now(),
    };

    let options = IcsOptions::default();
    let generator = IcsGenerator::new(options);
    let ics_content = generator.generate(&response).expect("生成ICS失败");

    println!("地理位置ICS内容:");
    for line in ics_content.lines() {
        if line.starts_with("LOCATION:")
            || line.starts_with("X-APPLE-STRUCTURED-LOCATION")
            || line.starts_with("GEO:")
        {
            println!("{}", line);
        }
    }

    // 验证包含地理位置信息
    assert!(ics_content.contains("LOCATION:重庆邮电大学第四教学楼"));
    assert!(ics_content.contains("X-APPLE-STRUCTURED-LOCATION"));
    assert!(ics_content.contains("GEO:29.536107;106.608759"));
}

#[test]
fn test_various_locations() {
    let options = IcsOptions::default();
    let generator = IcsGenerator::new(options);

    // 测试各种位置
    let test_cases = vec![
        ("2108", "重庆邮电大学二教学楼"),
        ("YF301", "重庆邮电大学-逸夫科技楼"),
        ("SL101", "重庆邮电大学数理学院"),
        ("风华运动场", "风华运动场"),
        ("太极体育场", "重庆邮电大学-太极体育场"),
        ("乒乓球馆", "风雨操场(乒乓球馆)"),
        ("篮球馆", "重庆邮电学院篮球排球馆"),
        ("仙桃A08", "重庆仙桃数据谷A08"),
        ("1234", "重庆邮电大学-光电工程学院"),
        ("8301", "重庆邮电大学八教学楼A栋"),
    ];

    for (location_input, expected_building) in test_cases {
        let geo_location = generator
            .location_manager
            .get_location_with_geo(location_input);
        println!(
            "输入: {} -> {}",
            location_input,
            geo_location.lines().next().unwrap_or("")
        );
        assert!(
            geo_location.contains(expected_building),
            "位置 {} 应该包含 {}",
            location_input,
            expected_building
        );
        assert!(
            geo_location.contains("GEO:"),
            "位置 {} 应该包含地理坐标",
            location_input
        );
    }
}

#[test]
fn test_exam_description() {
    // 测试考试描述格式
    let mut extra_exam = HashMap::new();
    extra_exam.insert("exam_type".to_string(), "期中".to_string());
    extra_exam.insert("status".to_string(), "有资格".to_string());
    extra_exam.insert("seat".to_string(), "B042".to_string());
    extra_exam.insert("week".to_string(), "8".to_string());

    let exam_course = Course {
        name: "测试课程".to_string(),
        code: Some("TEST001".to_string()),
        teacher: None,
        location: Some("3210".to_string()),
        start_time: chrono::DateTime::parse_from_rfc3339("2024-04-26T11:30:00Z")
            .unwrap()
            .with_timezone(&Utc),
        end_time: chrono::DateTime::parse_from_rfc3339("2024-04-26T13:30:00Z")
            .unwrap()
            .with_timezone(&Utc),
        description: None,
        course_type: Some("考试".to_string()),
        credits: None,
        recurrence: None,
        extra: extra_exam,
    };

    let options = IcsOptions::default();
    let generator = IcsGenerator::new(options);
    let description = generator.build_exam_description(&exam_course);

    assert!(description.contains("考试在第8周进行"));
    assert!(description.contains("时间为11:30至13:30"));
    assert!(description.contains("考试座位号是B042"));
    assert!(description.contains("考试状态: 有资格"));
    assert!(description.contains("祝考试顺利"));
}

#[test]
fn test_class_description() {
    // 测试普通课程描述格式
    let mut extra_class = HashMap::new();
    extra_class.insert("course_num".to_string(), "PHY300001".to_string());
    extra_class.insert("raw_week".to_string(), "1,3,5,7,9,11,13,15".to_string());
    extra_class.insert("current_week".to_string(), "5".to_string());

    let class_course = Course {
        name: "物理课程".to_string(),
        code: Some("PHY300001".to_string()),
        teacher: Some("王老师".to_string()),
        location: Some("2101".to_string()),
        start_time: Utc::now(),
        end_time: Utc::now(),
        description: None,
        course_type: Some("选修".to_string()),
        credits: Some(3.0),
        recurrence: None,
        extra: extra_class,
    };

    let options = IcsOptions::default();
    let generator = IcsGenerator::new(options);
    let description = generator.build_class_description(&class_course);

    assert!(description.contains("PHY300001"));
    assert!(description.contains("任课教师: 王老师"));
    assert!(description.contains("该课程是选修课"));
    assert!(description.contains("在1、3、5、7、9、11、13、15行课"));
    assert!(description.contains("当前是第5周"));
}

#[test]
fn test_course_title_format() {
    let options = IcsOptions::default();
    let generator = IcsGenerator::new(options);

    // 测试普通课程标题
    let class_course = Course {
        name: "数据结构".to_string(),
        code: Some("CS200001".to_string()),
        teacher: Some("陈老师".to_string()),
        location: Some("2105".to_string()),
        start_time: Utc::now(),
        end_time: Utc::now(),
        description: None,
        course_type: Some("必修".to_string()),
        credits: Some(4.0),
        recurrence: None,
        extra: HashMap::new(),
    };

    let class_title = generator.build_course_title(&class_course);
    assert_eq!(class_title, "数据结构 - 2105");

    // 测试考试标题
    let mut extra_exam = HashMap::new();
    extra_exam.insert("exam_type".to_string(), "期末".to_string());

    let exam_course = Course {
        name: "数据结构".to_string(),
        code: Some("CS200001".to_string()),
        teacher: None,
        location: Some("2105".to_string()),
        start_time: Utc::now(),
        end_time: Utc::now(),
        description: None,
        course_type: Some("考试".to_string()),
        credits: None,
        recurrence: None,
        extra: extra_exam,
    };

    let exam_title = generator.build_course_title(&exam_course);
    assert_eq!(exam_title, "[期末考试] 数据结构 - 2105");
}
