use thiserror::Error;
use actix_web::{ResponseError, HttpResponse, http::StatusCode};
use serde_json::json;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Authentication error: {0}")]
    AuthError(#[from] AuthError),
    
    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] WebSocketError),
    
    #[error("Proxy error: {0}")]
    ProxyError(#[from] ProxyError),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] DatabaseError),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),
}

// Implement conversion from config::ConfigError
impl From<config::ConfigError> for AppError {
    fn from(err: config::ConfigError) -> Self {
        AppError::ConfigError(err.to_string())
    }
}

// Implement conversion from sqlx::Error
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::DatabaseError(DatabaseError::NotFound),
            _ => AppError::DatabaseError(DatabaseError::QueryError(err.to_string())),
        }
    }
}

// Add conversion from std::io::Error
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::InternalError(err.to_string())
    }
}

// Implement actix_web::ResponseError for AppError
impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let message = self.to_string();
        let response = json!({
            "error": {
                "status": status.as_u16(),
                "message": message
            }
        });
        HttpResponse::build(status).json(response)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::AuthError(e) => match e {
                AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,
                AuthError::TokenExpired => StatusCode::UNAUTHORIZED,
                AuthError::InvalidToken => StatusCode::UNAUTHORIZED,
                AuthError::Unauthorized => StatusCode::FORBIDDEN,
                AuthError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            },
            AppError::ValidationError(_) => StatusCode::BAD_REQUEST,
            AppError::ConfigError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DatabaseError(DatabaseError::NotFound) => StatusCode::NOT_FOUND,
            AppError::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    
    #[error("Token expired")]
    TokenExpired,
    
    #[error("Invalid token")]
    InvalidToken,
    
    #[error("Unauthorized")]
    Unauthorized,
    
    #[error("Rate limited")]
    RateLimited,
}

#[derive(Error, Debug)]
pub enum WebSocketError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Message sending failed: {0}")]
    SendError(String),
    
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),
}

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("API request failed: {0}")]
    RequestFailed(String),
    
    #[error("Invalid API key")]
    InvalidApiKey,
    
    #[error("API rate limited")]
    RateLimited,
    
    #[error("API response error: {0}")]
    ResponseError(String),
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Query error: {0}")]
    QueryError(String),
    
    #[error("Record not found")]
    NotFound,
    
    #[error("Duplicate record")]
    Duplicate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_conversion() {
        // Test IO error conversion
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let app_err: AppError = io_err.into();
        assert!(matches!(app_err, AppError::InternalError(_)));

        // Test config error conversion
        let config_err = config::ConfigError::NotFound(String::from("key not found"));
        let app_err: AppError = config_err.into();
        assert!(matches!(app_err, AppError::ConfigError(_)));

        // Test database error conversion
        let db_err = sqlx::Error::RowNotFound;
        let app_err: AppError = db_err.into();
        assert!(matches!(app_err, AppError::DatabaseError(DatabaseError::NotFound)));
    }

    #[test]
    fn test_error_status_codes() {
        // Test auth error status codes
        let err = AppError::AuthError(AuthError::InvalidCredentials);
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);

        let err = AppError::AuthError(AuthError::Unauthorized);
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);

        // Test validation error status code
        let err = AppError::ValidationError("invalid input".to_string());
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);

        // Test database error status code
        let err = AppError::DatabaseError(DatabaseError::NotFound);
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_error_display() {
        let err = AppError::ValidationError("test error".to_string());
        assert_eq!(err.to_string(), "Validation error: test error");

        let err = AppError::AuthError(AuthError::InvalidCredentials);
        assert_eq!(err.to_string(), "Authentication error: Invalid credentials");

        let err = AppError::DatabaseError(DatabaseError::NotFound);
        assert_eq!(err.to_string(), "Database error: Record not found");
    }
}