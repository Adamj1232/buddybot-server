use actix_web::{web, App, HttpResponse, HttpServer};
use actix_cors::Cors;
use buddybot_server::{AppState, Settings, Result, AppError};
use dotenv::dotenv;
use std::net::TcpListener;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Health check endpoint handler
/// Returns a JSON response with server status and timestamp
async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

#[actix_web::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv().ok();
    
    // Initialize logging
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .pretty()
        .init();
    
    // Load configuration
    let config = Settings::new()?;
    info!("Configuration loaded successfully");
    
    info!("Starting server at {}:{}", config.server.host, config.server.port);
    
    // Initialize application state
    let state = AppState::new(config.clone()).await?;
    let state = web::Data::new(state);
    
    // Create and bind TCP listener
    let listener = TcpListener::bind(format!("{}:{}", config.server.host, config.server.port))?;
    
    // Start HTTP server
    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(state.clone())
            .route("/health", web::get().to(health_check))
    })
    .listen(listener)?
    .workers(config.server.workers as usize)
    .run()
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(())
}