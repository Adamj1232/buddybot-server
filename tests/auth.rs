use buddybot_server::{
    auth::{AuthService, RateLimiter, RateLimitConfig},
    db::DbOperations,
    error::Error,
};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/buddybot_test".to_string());
    
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn test_auth_flow() {
    // Setup test database
    let pool = setup_test_db().await;
    let db = DbOperations::new(std::sync::Arc::new(pool));
    
    // Create auth service
    let auth_service = AuthService::new(
        db,
        "test_secret".to_string(),
    );

    // Test authentication flow
    let token = auth_service.authenticate("test@example.com", "password123").await.unwrap();
    
    // Validate token
    let user = auth_service.validate_token(&token).await.unwrap();
    assert_eq!(user.email, "test@example.com");
}

#[tokio::test]
async fn test_rate_limiting() {
    let config = RateLimitConfig::default();
    let limiter = RateLimiter::new(config);
    let user_id = Uuid::new_v4();

    // Test standard tier limits
    for _ in 0..100 {
        assert!(limiter.check_rate_limit(user_id, "standard").await);
    }
    assert!(!limiter.check_rate_limit(user_id, "standard").await);

    // Test premium tier limits
    let premium_user_id = Uuid::new_v4();
    for _ in 0..500 {
        assert!(limiter.check_rate_limit(premium_user_id, "premium").await);
    }
    assert!(!limiter.check_rate_limit(premium_user_id, "premium").await);
}

#[tokio::test]
async fn test_invalid_token() {
    let pool = setup_test_db().await;
    let db = DbOperations::new(std::sync::Arc::new(pool));
    
    let auth_service = AuthService::new(
        db,
        "test_secret".to_string(),
    );

    match auth_service.validate_token("invalid_token").await {
        Err(Error::Unauthorized(_)) => (),
        _ => panic!("Expected unauthorized error"),
    }
} 