use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use cqupt_ics_core::{
    cache::CacheBackend, ics::IcsGenerator, location::LocationManager, prelude::*,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration as StdDuration};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::cache::RedisCache;

const DEFAULT_HOLIDAY_URL: &str = "https://calendars.icloud.com/holidays/cn_zh.ics";
const HOLIDAY_CACHE_KEY: &str = "holiday:cn_zh";
const HOLIDAY_CACHE_TTL: StdDuration = StdDuration::from_secs(60 * 60 * 24 * 30);

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub location_manager: &'static LocationManager,
    pub registry: &'static ProviderRegistry,
    pub holiday_calendar: &'static HolidayCalendar,
}

/// 健康检查响应
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

/// 错误响应
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

/// 课程获取请求参数
#[derive(Deserialize)]
struct GetCoursesQuery {
    provider: String,
    username: String,
    password: String,
    start_date: Option<String>, // 格式：YYYY-MM-DD，如 2024-03-04，可选
    format: Option<String>,     // "json" or "ics"，默认为 "ics"
}

pub async fn create_app(
    redis_manager: &redis::aio::ConnectionManager,
    registry: cqupt_ics_core::prelude::ProviderRegistry,
) -> Result<Router, AppError> {
    let registry = registry.into_static();
    let location_manager = LocationManager::new().into_static();
    let http_client = Client::builder()
        .user_agent("cqupt-ics-server/holiday-loader")
        .build()
        .expect("Failed to create HTTP client");

    let holiday_cache = RedisCache::new("cqupt-ics".to_string(), redis_manager.clone());

    let holiday_calendar = load_holiday_calendar(http_client, &holiday_cache)
        .await?
        .into_static();

    let state: AppState = AppState {
        location_manager,
        registry,
        holiday_calendar,
    };

    let router = Router::new()
        .route("/api", get(root_handler))
        .route("/health", get(health_handler))
        .route("/courses", get(get_courses_handler))
        .route("/providers", get(list_providers_handler))
        .route("/locations", get(list_locations_handler))
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        );

    Ok(router)
}

/// 根路径处理器
async fn root_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "CQUPT ICS Calendar Service",
        "version": "0.1.0",
        "description": "Rust implementation of CQUPT course schedule export service",
        "endpoints": {
            "health": "/health",
            "courses": "/courses",
            "providers": "/providers",
            "locations": "/locations"
        }
    }))
}

/// 健康检查处理器
async fn health_handler() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// 列出所有provider
async fn list_providers_handler(State(state): State<AppState>) -> impl IntoResponse {
    let providers: Vec<_> = state
        .registry
        .list_providers()
        .map(|(name, description)| {
            serde_json::json!({
                "name": name,
                "description": description,
                "status": "available"
            })
        })
        .collect();

    Json(serde_json::json!({
        "providers": providers
    }))
}

/// 列出位置映射
async fn list_locations_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mappings: Vec<_> = state
        .location_manager
        .get_all_mappings()
        .values()
        .cloned()
        .collect();
    Json(mappings)
}

/// 获取课程处理器
async fn get_courses_handler(
    Query(params): Query<GetCoursesQuery>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    use std::collections::HashMap;

    let semester = params
        .start_date
        .map(|date_str| {
            tracing::info!("使用指定的学期开始日期: {}", date_str);
            Semester::from_date_str(&date_str)
                .map_err(|e| cqupt_ics_core::Error::Config(format!("Invalid start date: {}", e)))
        })
        .transpose()?;
    // 创建请求对象
    let mut request = CourseRequest {
        credentials: Credentials {
            username: params.username.clone(),
            password: params.password,
            extra: HashMap::new(),
        },
        semester,
    };

    // 获取 provider
    let provider = state
        .registry
        .get_provider(&params.provider)
        .ok_or_else(|| {
            AppError(cqupt_ics_core::Error::Config(format!(
                "Unknown provider: {}",
                params.provider
            )))
        })?;

    // 获取课程数据
    let mut response = provider.get_courses(&mut request).await?;

    state.holiday_calendar.apply_to_response(&mut response);

    // 根据格式参数返回不同内容，默认为 ics
    match params.format.as_deref() {
        Some("json") => {
            // 返回JSON格式
            Ok(Json(response).into_response())
        }
        _ => {
            // 默认返回ICS格式
            let options = IcsOptions {
                calendar_name: Some(format!("CQUPT课程表-{}", params.username)),
                include_teacher: true,
                reminder_minutes: Some(15),
                ..Default::default()
            };
            let generator = IcsGenerator::new(options);
            let ics_content = generator.generate(&response)?;

            Ok((
                StatusCode::OK,
                [("Content-Type", "text/calendar; charset=utf-8")],
                ics_content,
            )
                .into_response())
        }
    }
}

async fn load_holiday_calendar(
    client: Client,
    holiday_cache: &RedisCache,
) -> Result<HolidayCalendar, AppError> {
    if let Some(bytes) = holiday_cache.get_raw(HOLIDAY_CACHE_KEY).await? {
        tracing::debug!("命中节假日调休缓存");
        return HolidayCalendar::from_bytes(&bytes).map_err(AppError::from);
    }

    let url = std::env::var("HOLIDAY_ICS_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_HOLIDAY_URL.to_string());

    tracing::info!("使用节假日调休日历: {}", url);

    let response = client.get(&url).send().await.map_err(|e| {
        AppError(cqupt_ics_core::Error::Provider {
            provider: "holiday".to_string(),
            message: format!("请求节假日ICS失败: {}", e),
        })
    })?;

    if !response.status().is_success() {
        return Err(AppError(cqupt_ics_core::Error::Provider {
            provider: "holiday".to_string(),
            message: format!("获取节假日ICS失败: HTTP {}", response.status()),
        }));
    }

    let bytes = response.bytes().await.map_err(|e| {
        AppError(cqupt_ics_core::Error::Provider {
            provider: "holiday".to_string(),
            message: format!("读取节假日ICS内容失败: {}", e),
        })
    })?;
    let data = bytes.to_vec();

    holiday_cache
        .set_raw(HOLIDAY_CACHE_KEY, &data, HOLIDAY_CACHE_TTL)
        .await?;
    HolidayCalendar::from_bytes(&data).map_err(AppError::from)
}

/// 应用错误类型
#[derive(Debug)]
pub(crate) struct AppError(cqupt_ics_core::Error);

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 复用内部错误的 Display
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self.0 {
            cqupt_ics_core::Error::Config(_) => (StatusCode::BAD_REQUEST, "配置错误"),
            cqupt_ics_core::Error::Authentication(_) => (StatusCode::UNAUTHORIZED, "认证失败"),
            cqupt_ics_core::Error::Provider { .. } => (StatusCode::BAD_GATEWAY, "provider错误"),
            cqupt_ics_core::Error::Timeout => (StatusCode::GATEWAY_TIMEOUT, "请求超时"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "内部服务器错误"),
        };

        let body = Json(ErrorResponse {
            error: error_message.to_string(),
            message: self.0.to_string(),
        });

        (status, body).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<cqupt_ics_core::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
