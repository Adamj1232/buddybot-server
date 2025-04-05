pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod proxy;
pub mod scaling;
pub mod websocket;

use std::sync::Arc;
use sqlx::PgPool;
use actix_web::HttpResponse;

pub use error::AppError;
pub type Result<T> = std::result::Result<T, AppError>;
pub use config::Settings;

pub use auth::{AuthService, RateLimiter, RateLimitConfig};
pub use db::{DbOperations, User, UserSession};

/// Health check endpoint handler
/// Returns a JSON response with server status and timestamp
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// Application state shared across all components
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Settings>,
    pub db_pool: Arc<PgPool>,
}

impl AppState {
    pub async fn new(config: Settings) -> Result<Self> {
        // Initialize database connection pool
        let db_pool = PgPool::connect(&config.database.url)
            .await
            .map_err(|e| AppError::DatabaseError(error::DatabaseError::ConnectionError(e.to_string())))?;

        Ok(Self {
            config: Arc::new(config),
            db_pool: Arc::new(db_pool),
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        // Close database connections
        self.db_pool.close().await;
        
        // Additional cleanup can be added here
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    use std::env;

    fn cleanup_env() {
        env::remove_var("APP_DATABASE__URL");
    }

    #[tokio::test]
    async fn test_app_state_creation() {
        cleanup_env();
        let config = Settings::new_for_test().expect("Failed to load test config");
        let state = AppState::new(config).await;
        
        // Since we don't have a test database configured, this should fail
        assert!(state.is_err());
        if let Err(e) = state {
            assert!(matches!(e, AppError::DatabaseError(_)));
        }
    }

    #[tokio::test]
    async fn test_app_state_clone() {
        cleanup_env();
        let config = Settings::new_for_test().expect("Failed to load test config");
        
        // Create a mock PgPool (since we can't connect to real DB in tests)
        let pool = PgPool::connect("postgres://postgres:postgres@localhost/postgres")
            .await
            .expect("Failed to create mock pool");
        
        let state = AppState {
            config: Arc::new(config),
            db_pool: Arc::new(pool),
        };
        
        let cloned = state.clone();
        
        // Verify Arc references are shared
        assert!(Arc::ptr_eq(&state.config, &cloned.config));
        assert!(Arc::ptr_eq(&state.db_pool, &cloned.db_pool));
    }
} 