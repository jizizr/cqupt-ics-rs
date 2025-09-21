use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{Client, ClientBuilder};

use crate::{CourseRequest, CourseResponse, Error};

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
        CourseResponse {
            courses: Vec::new(),
            semester: request.semester.clone().unwrap_or_else(|| crate::Semester {
                start_date: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            }),
            generated_at: Utc::now(),
        }
    }
}
