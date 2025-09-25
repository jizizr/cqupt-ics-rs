use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use cqupt_ics_core::{ics::IcsGenerator, location::LocationManager, prelude::*};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::registry;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub location_manager: Arc<LocationManager>,
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

pub async fn create_app() -> Router {
    let location_manager = Arc::new(LocationManager::default());
    let state = AppState { location_manager };

    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .route("/courses", get(get_courses_handler))
        .route("/providers", get(list_providers_handler))
        .route("/locations", get(list_locations_handler))
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
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
async fn list_providers_handler() -> impl IntoResponse {
    let providers: Vec<_> = registry::list_providers()
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
    State(_state): State<AppState>,
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
    let provider = registry::get_provider(&params.provider).ok_or_else(|| {
        cqupt_ics_core::Error::Config(format!("Provider '{}' not found", params.provider))
    })?;

    // 获取课程数据
    let response = provider.get_courses(&mut request).await?;

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

/// 应用错误类型
#[derive(Debug)]
struct AppError(cqupt_ics_core::Error);

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
