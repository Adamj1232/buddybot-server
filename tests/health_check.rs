use actix_web::{test, App, web};
use buddybot_server::{AppState, Settings};
use chrono::DateTime;

#[actix_web::test]
async fn test_health_check() {
    // Create test app state
    let config = Settings::new().expect("Failed to load test config");
    let state = web::Data::new(AppState {
        config: std::sync::Arc::new(config),
        db_pool: std::sync::Arc::new(
            sqlx::PgPool::connect("postgres://fake:fake@localhost/fake")
                .await
                .expect("Failed to create mock pool")
        ),
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