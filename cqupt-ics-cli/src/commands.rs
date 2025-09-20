use std::{collections::HashMap, fs};

use anyhow::Result;
use chrono::Utc;
use cqupt_ics_core::{
    ics::IcsGenerator, location::LocationManager, prelude::*, semester::SemesterDetector,
};

use crate::registry;

/// 生成课程表命令参数
pub struct GenerateParams {
    pub provider_name: String,
    pub username: String,
    pub password: String,
    pub year: Option<u32>,
    pub term: Option<u32>,
    pub output: Option<String>,
    pub calendar_name: Option<String>,
    pub include_teacher: bool,
    pub reminder_minutes: u32,
}

/// 生成课程表命令
pub async fn generate_command(params: GenerateParams) -> Result<()> {
    // 自动检测当前学期（如果未提供年份和学期）
    let (actual_year, actual_term) = match (params.year, params.term) {
        (Some(year), Some(term)) => {
            // 用户手动指定了年份和学期
            tracing::info!("使用用户指定的学期: {}-{}", year, term);
            (year, term)
        }
        (year_opt, term_opt) => {
            // 自动检测当前学期
            let (detected_year, detected_term, semester_type) = SemesterDetector::detect_current();
            let final_year = year_opt.unwrap_or(detected_year);
            let final_term = term_opt.unwrap_or(detected_term);

            tracing::info!(
                "自动检测学期: {}-{} ({:?}), 最终使用: {}-{}",
                detected_year,
                detected_term,
                semester_type,
                final_year,
                final_term
            );
            println!(
                "自动检测当前学期: {}-{} ({:?})",
                detected_year, detected_term, semester_type
            );

            (final_year, final_term)
        }
    };

    tracing::info!(
        "开始生成课程表: provider={}, 用户={}, 学年={}, 学期={}",
        params.provider_name,
        params.username,
        actual_year,
        actual_term
    );

    // 创建准确的学期对象
    let semester = SemesterDetector::create_semester(actual_year, actual_term)
        .map_err(|e| anyhow::anyhow!("创建学期失败: {}", e))?;

    // 创建请求对象
    let request = CourseRequest {
        credentials: Credentials {
            username: params.username.clone(),
            password: params.password,
            extra: HashMap::new(),
        },
        semester,
        provider_config: ProviderConfig {
            name: params.provider_name.clone(),
            base_url: String::new(),
            timeout: Some(30),
            extra: HashMap::new(),
        },
    };

    let provider = registry::get_provider(&params.provider_name)
        .ok_or_else(|| anyhow::anyhow!("未知的provider: {}", params.provider_name))?;
    // 获取课程数据
    println!("验证用户凭据...");
    let response = provider.get_courses(&request).await?;
    println!("✓ 凭据验证成功");
    println!("✓ 成功获取 {} 门课程", response.courses.len());
    // 生成ICS文件
    println!("生成ICS日历文件...");
    let options = IcsOptions {
        calendar_name: params
            .calendar_name
            .or_else(|| Some(format!("{}的课程表", params.username))),
        timezone: Some("Asia/Shanghai".to_string()),
        include_description: true,
        include_teacher: params.include_teacher,
        reminder_minutes: Some(params.reminder_minutes),
    };

    let generator = IcsGenerator::new(options);
    let ics_content = generator.generate(&response)?;

    // 确定输出文件名
    let output_file = params.output.unwrap_or_else(|| {
        format!(
            "cqupt-schedule-{}-{}-{}.ics",
            params.username, actual_year, actual_term
        )
    });

    // 写入文件
    fs::write(&output_file, ics_content)?;
    println!("✓ ICS文件已保存到: {}", output_file);

    Ok(())
}

/// 验证凭据命令
pub async fn validate_command(
    provider_name: String,
    username: String,
    password: String,
) -> Result<()> {
    tracing::info!("验证凭据: provider={}, 用户={}", provider_name, username);

    let request = CourseRequest {
        credentials: Credentials {
            username: username.clone(),
            password,
            extra: HashMap::new(),
        },
        semester: Semester {
            year: 2024,
            term: 1,
            start_date: Utc::now(),
            end_date: Utc::now(),
        },
        provider_config: ProviderConfig {
            name: provider_name.clone(),
            base_url: String::new(),
            timeout: Some(30),
            extra: HashMap::new(),
        },
    };

    let provider = registry::get_provider(&provider_name)
        .ok_or_else(|| anyhow::anyhow!("未知的provider: {}", provider_name))?;

    // 获取课程数据
    println!("验证用户凭据...");
    provider.validate(&request).await?;
    println!("凭据验证成功");

    Ok(())
}

/// 列出provider命令
pub async fn providers_command() -> Result<()> {
    println!("可用的数据provider:");

    let providers: Vec<_> = registry::list_providers().collect();

    if providers.is_empty() {
        println!("  暂无可用的数据provider");
    } else {
        for (name, description) in providers {
            println!("  {} - {}", name, description);
        }
    }

    Ok(())
}

/// 列出位置映射命令
pub async fn location_list_command() -> Result<()> {
    let manager = LocationManager::default();
    let mappings = manager.get_all_mappings();

    println!("位置映射列表:");
    for (original, mapping) in mappings {
        println!("  {} -> {}", original, mapping.normalized);
        if let Some(ref building) = mapping.building {
            println!("    建筑: {}", building);
        }
        if let Some(ref campus) = mapping.campus {
            println!("    校区: {}", campus);
        }
    }

    Ok(())
}

/// 标准化位置名称命令
pub async fn location_normalize_command(location: String) -> Result<()> {
    let manager = LocationManager::default();
    let normalized = manager.normalize_location(&location);

    println!("原始位置: {}", location);
    println!("标准化位置: {}", normalized);

    if let Some(details) = manager.get_location_details(&location) {
        println!("详细信息:");
        if let Some(ref building) = details.building {
            println!("  建筑: {}", building);
        }
        if let Some(ref room) = details.room {
            println!("  房间: {}", room);
        }
        if let Some(ref campus) = details.campus {
            println!("  校区: {}", campus);
        }
    }

    Ok(())
}

/// 导入位置映射命令
pub async fn location_import_command(file: String) -> Result<()> {
    let content = fs::read_to_string(&file)?;
    let mut manager = LocationManager::new();
    manager.load_from_json(&content)?;

    println!(
        "✓ 成功从 {} 导入 {} 个位置映射",
        file,
        manager.get_all_mappings().len()
    );

    Ok(())
}

/// 导出位置映射命令
pub async fn location_export_command(file: String) -> Result<()> {
    let manager = LocationManager::default();
    let json_content = manager.export_to_json()?;

    fs::write(&file, json_content)?;
    println!("✓ 位置映射已导出到: {}", file);

    Ok(())
}
