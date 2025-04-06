use actix_web::{test, web, App};
use buddybot_server::{AppState, Settings, auth::handlers::{login, register}};
use serde_json::json;

#[actix_web::test]
async fn test_register_and_login() {
    let config = Settings::new().unwrap();
    let state = AppState::new(config.clone()).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/auth/login", web::post().to(login))
            .route("/auth/register", web::post().to(register))
    ).await;

    // Test registration
    let register_response = test::TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({
            "email": "test@example.com",
            "password": "password123",
            "display_name": "Test User"
        }))
        .send_request(&app)
        .await;
    
    assert_eq!(register_response.status(), 201);
    let register_body: serde_json::Value = test::read_body_json(register_response).await;
    assert!(register_body.get("token").is_some());

    // Test login
    let login_response = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({
            "email": "test@example.com",
            "password": "password123"
        }))
        .send_request(&app)
        .await;
    
    assert_eq!(login_response.status(), 200);
    let login_body: serde_json::Value = test::read_body_json(login_response).await;
    assert!(login_body.get("token").is_some());
}

#[actix_web::test]
async fn test_invalid_login() {
    let config = Settings::new().unwrap();
    let state = AppState::new(config.clone()).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/auth/login", web::post().to(login))
            .route("/auth/register", web::post().to(register))
    ).await;
    
    let response = test::TestRequest::post()
        .uri("/auth/login")
        .set_json(json!({
            "email": "nonexistent@example.com",
            "password": "wrongpassword"
        }))
        .send_request(&app)
        .await;
    
    assert_eq!(response.status(), 401);
}

#[actix_web::test]
async fn test_invalid_registration() {
    let config = Settings::new().unwrap();
    let state = AppState::new(config.clone()).await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/auth/login", web::post().to(login))
            .route("/auth/register", web::post().to(register))
    ).await;
    
    let response = test::TestRequest::post()
        .uri("/auth/register")
        .set_json(json!({
            "email": "test@example.com",
            "password": ""  // Empty password should fail
        }))
        .send_request(&app)
        .await;
    
    assert_eq!(response.status(), 401);
} 