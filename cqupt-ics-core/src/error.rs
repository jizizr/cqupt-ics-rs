use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parsing failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Date/time parsing failed: {0}")]
    DateTime(#[from] chrono::ParseError),

    #[error("Provider error: {provider} - {message}")]
    Provider { provider: String, message: String },

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("ICS generation failed: {0}")]
    IcsGeneration(String),

    #[error("Location not found: {0}")]
    LocationNotFound(String),

    #[error("Authentication failed for provider: {0}")]
    Authentication(String),

    #[error("Network timeout")]
    Timeout,

    #[error("RSA Error: {0}")]
    Rsa(#[from] rsa::errors::Error),

    #[error("学校网络宵禁时间")]
    CurfewTime(()),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;
