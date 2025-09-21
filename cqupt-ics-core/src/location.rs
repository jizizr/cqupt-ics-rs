use std::collections::HashMap;

use regex::Regex;
use serde_json;

use crate::{LocationMapping, Result};

/// 位置管理器
pub struct LocationManager {
    mappings: HashMap<String, LocationMapping>,
}

impl LocationManager {
    /// 创建新的位置管理器
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    /// 从JSON字符串加载位置映射
    pub fn load_from_json(&mut self, json_data: &str) -> Result<()> {
        let mappings: Vec<LocationMapping> = serde_json::from_str(json_data)?;

        for mapping in mappings {
            self.mappings.insert(mapping.original.clone(), mapping);
        }

        Ok(())
    }

    /// 添加位置映射
    pub fn add_mapping(&mut self, mapping: LocationMapping) {
        self.mappings.insert(mapping.original.clone(), mapping);
    }

    /// 标准化位置名称
    pub fn normalize_location(&self, original: &str) -> String {
        // 首先尝试精确匹配
        if let Some(mapping) = self.mappings.get(original) {
            return mapping.normalized.clone();
        }

        // 如果没有精确匹配，尝试模糊匹配
        for mapping in self.mappings.values() {
            if original.contains(&mapping.original) || mapping.original.contains(original) {
                return mapping.normalized.clone();
            }
        }

        // 如果都没有匹配，进行基本的清理
        self.basic_normalize(original)
    }

    /// 基本的位置名称清理
    fn basic_normalize(&self, location: &str) -> String {
        location
            .trim()
            .replace("  ", " ") // 移除多余空格
            .replace("教学楼", "")
            .replace("实验楼", "")
            .replace("综合楼", "")
            .trim()
            .to_string()
    }

    pub fn get_location_details(&self, original: &str) -> Option<&LocationMapping> {
        self.mappings.get(original)
    }

    pub fn get_all_mappings(&self) -> &HashMap<String, LocationMapping> {
        &self.mappings
    }

    pub fn export_to_json(&self) -> Result<String> {
        let mappings: Vec<&LocationMapping> = self.mappings.values().collect();
        Ok(serde_json::to_string_pretty(&mappings)?)
    }

    /// 根据位置生成带有地理坐标的ICS位置信息
    /// 对应Python中的get_location函数
    pub fn get_location_with_geo(&self, loc: &str) -> String {
        // 提取四位数教室号
        let room = self.extract_room_number(loc);

        let custom_geo = if loc.contains("YF") {
            r#"LOCATION:重庆邮电大学-逸夫科技楼\n崇文路2号重庆邮电大学
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学-逸夫科技楼\\n崇文路2号重庆邮电大学:geo:29.535617,106.607390"#
        } else if loc.contains("SL") {
            r#"LOCATION:重庆邮电大学数理学院\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学数理学院\\n崇文路2号重庆邮电大学内:geo:29.530599,106.605454"#
        } else if loc.contains("综合实验") || loc.contains("实验实训室") {
            r#"LOCATION:重庆邮电大学综合实验大楼\n南山路新力村
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学综合实验大楼\\n南山路新力村:geo:29.524289,106.605595"#
        } else if loc.contains("风华") || loc == "运动场1" {
            r#"LOCATION:风华运动场\n南山街道重庆邮电大学5栋
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=风华运动场\\n南山街道重庆邮电大学5栋:geo:29.532757,106.607510"#
        } else if loc.contains("太极") {
            r#"LOCATION:重庆邮电大学-太极体育场\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学-太极体育场\\n崇文路2号重庆邮电大学内:geo:29.532940,106.609072"#
        } else if loc.contains("乒乓球") {
            r#"LOCATION:风雨操场(乒乓球馆)\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=风雨操场(乒乓球馆)\\n崇文路2号重庆邮电大学内:geo:29.534230,106.608516"#
        } else if loc.contains("篮球") || loc.contains("排球") {
            r#"LOCATION:重庆邮电学院篮球排球馆\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电学院篮球排球馆\\n崇文路2号重庆邮电大学内:geo:29.534025,106.609148"#
        } else if loc.contains("仙桃A08") {
            r#"LOCATION:重庆仙桃数据谷A08\n中国重庆市渝北区金山大道仙桃国际大数据谷体验中心
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆仙桃数据谷A08\\n中国重庆市渝北区金山大道仙桃国际大数据谷体验中心:geo:29.739791,106.55661"#
        } else if loc.contains("仙桃运动场") {
            r#"LOCATION:仙桃体育公园\n中国重庆市渝北区金山大道仙桃国际大数据谷体验中心
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=仙桃体育公园\\n中国重庆市渝北区仙桃街道数据谷东路仙桃国际数据谷内:geo:29.745789,106.55749"#
        } else if room.starts_with('1') {
            r#"LOCATION:重庆邮电大学-光电工程学院\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学-光电工程学院\\n崇文路2号重庆邮电大学内:geo:29.531478,106.605921"#
        } else if room.starts_with('2') {
            r#"LOCATION:重庆邮电大学二教学楼\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学二教学楼\\n崇文路2号重庆邮电大学内:geo:29.532703,106.606747"#
        } else if room.starts_with('3') {
            r#"LOCATION:重庆邮电大学第三教学楼\n崇文路2号
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学第三教学楼\\n崇文路2号:geo:29.535119,106.609114"#
        } else if room.starts_with('4') {
            r#"LOCATION:重庆邮电大学第四教学楼\n崇文路2号
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学第四教学楼\\n崇文路2号:geo:29.536107,106.608759"#
        } else if room.starts_with('5') {
            r#"LOCATION:重庆邮电大学-国际学院\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学-国际学院\\n崇文路2号重庆邮电大学内:geo:29.536131,106.610090"#
        } else if room.starts_with('8') {
            r#"LOCATION:重庆邮电大学八教学楼A栋\n崇文路2号重庆邮电大学内
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学八教学楼A栋\\n崇文路2号重庆邮电大学内:geo:29.535322,106.611020"#
        } else {
            // Fallback
            r#"LOCATION:重庆邮电大学\n崇文路2号
X-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=重庆邮电大学\\n崇文路2号:geo:29.530807,106.607617"#
        };

        // 提取geo坐标并格式化最终结果
        let geo_part = custom_geo
            .split("geo:")
            .nth(1)
            .unwrap_or("29.530807,106.607617")
            .replace(',', ";");

        let custom_geo_crlf = custom_geo.replace('\n', "\r\n");
        format!("{}\r\nGEO:{}\r\n", custom_geo_crlf, geo_part)
    }

    /// 提取四位数教室号
    fn extract_room_number(&self, loc: &str) -> String {
        let re = Regex::new(r"[0-9]{4}").unwrap();
        if let Some(captures) = re.find(loc) {
            captures.as_str().to_string()
        } else {
            "6666".to_string() // 不存在四位数以上的数字教室匹配
        }
    }
}

impl Default for LocationManager {
    fn default() -> Self {
        let mut manager = Self::new();

        // 添加一些默认的位置映射
        // TODO: 未来可以考虑简化教学楼输出
        manager.add_mapping(LocationMapping {
            original: "第一教学楼".to_string(),
            normalized: "一教".to_string(),
            building: Some("第一教学楼".to_string()),
            room: None,
            campus: Some("南山校区".to_string()),
        });

        manager.add_mapping(LocationMapping {
            original: "第二教学楼".to_string(),
            normalized: "二教".to_string(),
            building: Some("第二教学楼".to_string()),
            room: None,
            campus: Some("南山校区".to_string()),
        });

        manager.add_mapping(LocationMapping {
            original: "第三教学楼".to_string(),
            normalized: "三教".to_string(),
            building: Some("第三教学楼".to_string()),
            room: None,
            campus: Some("南山校区".to_string()),
        });

        manager.add_mapping(LocationMapping {
            original: "实验楼".to_string(),
            normalized: "实验楼".to_string(),
            building: Some("实验楼".to_string()),
            room: None,
            campus: Some("南山校区".to_string()),
        });

        manager
    }
}
