use crate::{CourseRequest, CourseResponse, Error, Result};
use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, de::Deserializer};
use std::time::Duration;

/// 基础provider结构
pub struct BaseProviderBuilder {
    pub client_builder: ClientBuilder,
    pub info: ProviderInfo,
}

pub struct BaseProvider {
    pub client: Client,
    pub info: ProviderInfo,
}

pub struct ProviderInfo {
    pub name: String,
    pub description: String,
}

impl BaseProviderBuilder {
    pub fn new(info: ProviderInfo) -> Self {
        let client_builder = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("CQUPT-ICS-Rust/0.1.0")
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert("Accept", "*/*".parse().unwrap());
                headers.insert("Content-Type", "application/json".parse().unwrap());
                headers.insert(
                    "Accept-Encoding",
                    "br;q=1.0, gzip;q=0.9, deflate;q=0.8".parse().unwrap(),
                );
                headers
            });

        Self {
            client_builder,
            info,
        }
    }

    pub fn new_with_timeout(info: ProviderInfo, timeout_secs: u64) -> Self {
        let mut s = Self::new(info);
        s.client_builder = s.client_builder.timeout(Duration::from_secs(timeout_secs));
        s
    }

    pub fn build(self) -> BaseProvider {
        let client = self
            .client_builder
            .build()
            .expect("Failed to create HTTP client");

        BaseProvider {
            client,
            info: self.info,
        }
    }
}

impl BaseProvider {
    /// 通用的错误处理
    pub fn handle_error_req(&self, error: reqwest::Error) -> Error {
        if error.is_timeout() {
            Error::Timeout
        } else if error.is_request() {
            Error::Provider {
                provider: self.info.name.clone(),
                message: format!("Request failed: {}", error),
            }
        } else {
            Error::Http(error)
        }
    }

    pub fn custom_error(&self, message: impl Into<String>) -> Error {
        Error::Provider {
            provider: self.info.name.clone(),
            message: message.into(),
        }
    }

    /// 创建空的课程响应
    pub fn empty_response(&self, request: &CourseRequest) -> CourseResponse {
        let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap(); // UTC+8
        CourseResponse {
            courses: Vec::new(),
            semester: request.semester.clone().unwrap(),
            generated_at: Utc::now().with_timezone(&tz),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Claims {
    #[serde(deserialize_with = "de_exp")]
    exp: u64,
    #[allow(dead_code)]
    sub: Option<String>,
}

fn de_exp<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Exp {
        N(u64),
        S(String),
    }
    match Exp::deserialize(deserializer)? {
        Exp::N(n) => Ok(n),
        Exp::S(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
    }
}

// 尝试多种 Base64 变体解码（URL_SAFE_NO_PAD -> URL_SAFE -> STANDARD -> STANDARD_NO_PAD）
fn decode_base64_flex(s: &str) -> Result<Vec<u8>> {
    // 先尝试 URL_SAFE_NO_PAD
    if let Ok(b) = general_purpose::URL_SAFE_NO_PAD.decode(s) {
        return Ok(b);
    }
    // URL_SAFE（允许 '='）
    if let Ok(b) = general_purpose::URL_SAFE.decode(s) {
        return Ok(b);
    }
    // STANDARD（含 '+' '/' '='）
    if let Ok(b) = general_purpose::STANDARD.decode(s) {
        return Ok(b);
    }
    // STANDARD_NO_PAD
    if let Ok(b) = general_purpose::STANDARD_NO_PAD.decode(s) {
        return Ok(b);
    }
    Err(Error::Authentication("Base64 decode failed".to_string()))
}

pub fn is_token_expired(token: &str) -> Result<bool> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() == 3 {
        // 标准 JWT：取中间段 payload
        let payload_b = decode_base64_flex(parts[1])?;
        let claims: Claims = serde_json::from_slice(&payload_b)?;
        let now = Utc::now().timestamp() as u64;
        Ok(claims.exp <= now)
    } else if parts.len() == 2 {
        // 非标准两段：通常第一段是 payload
        let payload_b = decode_base64_flex(parts[0])?;
        let claims: Claims = serde_json::from_slice(&payload_b)?;
        let now = Utc::now().timestamp() as u64;
        Ok(claims.exp <= now)
    } else {
        Err(Error::Authentication(
            "Token format not recognized (need 2 or 3 segments)".to_string(),
        ))
    }
}
