use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use cqupt_ics_core::{
    ics::IcsGenerator, location::LocationManager, prelude::*, semester::SemesterDetector,
};

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
    year: Option<u32>,
    term: Option<u32>,
    format: Option<String>, // "json" or "ics"
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

/// GET /courses 处理器
async fn get_courses_handler(
    Query(params): Query<GetCoursesQuery>,
    State(_state): State<AppState>,
) -> std::result::Result<Response, AppError> {
    tracing::info!(
        "GET /courses request: provider={}, username={}",
        params.provider,
        params.username
    );

    let username = params.username.clone(); // Clone to avoid move issues

    // 自动检测或使用指定的学期
    let (actual_year, actual_term) = match (params.year, params.term) {
        (Some(year), Some(term)) => (year, term),
        (year_opt, term_opt) => {
            let (detected_year, detected_term, _) = SemesterDetector::detect_current();
            let final_year = year_opt.unwrap_or(detected_year);
            let final_term = term_opt.unwrap_or(detected_term);
            tracing::info!("自动检测学期: {}-{}", final_year, final_term);
            (final_year, final_term)
        }
    };

    // 创建准确的学期对象
    let semester = SemesterDetector::create_semester(actual_year, actual_term)
        .map_err(|e| cqupt_ics_core::Error::Config(format!("创建学期失败: {}", e)))?;

    // 创建请求对象
    let request = CourseRequest {
        credentials: Credentials {
            username: params.username,
            password: params.password,
            extra: std::collections::HashMap::new(),
        },
        semester,
        provider_config: ProviderConfig {
            name: params.provider.clone(),
            base_url: "".to_string(),
            timeout: Some(30),
            extra: std::collections::HashMap::new(),
        },
    };

    // 创建providerwrapper
    let wrapper = registry::get_provider(&params.provider).ok_or_else(|| {
        cqupt_ics_core::Error::Config(format!("未知的provider: {}", params.provider))
    })?;

    // 获取课程数据
    let response = wrapper.get_courses(&request).await?;

    // 根据格式返回不同内容
    match params.format.as_deref().unwrap_or("ics") {
        "ics" => {
            let generator = IcsGenerator::default();
            let ics_content = generator.generate(&response)?;

            let mut headers = HeaderMap::new();
            headers.insert(
                "content-type",
                "text/calendar; charset=utf-8".parse().unwrap(),
            );
            headers.insert(
                "content-disposition",
                format!("attachment; filename=\"cqupt-schedule-{}.ics\"", username)
                    .parse()
                    .unwrap(),
            );

            Ok((headers, ics_content).into_response())
        }
        _ => Ok(Json(response).into_response()),
    }
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
