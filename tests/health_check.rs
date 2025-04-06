use actix_web::{test, App, web};
use buddybot_server::{AppState, Settings};
use chrono::DateTime;

#[actix_web::test]
async fn test_health_check() {
    // Create test app state
    let config = Settings::new().expect("Failed to load test config");
    let pool = std::sync::Arc::new(
        sqlx::PgPool::connect("postgres://fake:fake@localhost/fake")
            .await
            .expect("Failed to create mock pool")
    );
    let db_ops = buddybot_server::db::DbOperations::new(pool.clone());
    let state = web::Data::new(AppState {
        config: std::sync::Arc::new(config),
        db_pool: pool,
        scaling: std::sync::Arc::new(buddybot_server::scaling::ScalingManager::new(
            buddybot_server::scaling::ScalingConfig::default()
        )),
        auth_service: std::sync::Arc::new(buddybot_server::auth::AuthService::new(
            db_ops,
            "test_secret".to_string(),
        )),
    });

    // Create test app
    let app = test::init_service(
        App::new()
            .app_data(state.clone())
            .route("/health", web::get().to(buddybot_server::health_check))
    ).await;

    // Send request
    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;

    // Assert response
    assert!(resp.status().is_success());

    // Parse response body
    let body = test::read_body(resp).await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify response format
    assert_eq!(json["status"], "healthy");
    assert!(DateTime::parse_from_rfc3339(
        json["timestamp"].as_str().unwrap()
    ).is_ok());
} 