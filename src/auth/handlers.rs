use actix_web::{web, HttpResponse, HttpRequest};
use serde::{Deserialize, Serialize};
use crate::AppState;
use crate::error::Error;
use tracing::{info, error};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
}

pub async fn login(
    req: web::Json<LoginRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    info!("Received login request for email: {}", req.email);
    match state.auth_service.authenticate(&req.email, &req.password).await {
        Ok(token) => {
            info!("Login successful for email: {}", req.email);
            Ok(HttpResponse::Ok().json(AuthResponse { token }))
        }
        Err(e) => {
            error!("Login failed for email: {}: {}", req.email, e);
            Err(e)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

pub async fn register(
    req: web::Json<RegisterRequest>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    info!("Received registration request for email: {}", req.email);
    
    // Attempt registration
    match state.auth_service.register(
        &req.email,
        &req.password,
        req.display_name.as_deref(),
    ).await {
        Ok(_) => {
             info!("Registration successful for email: {}", req.email);
        }
        Err(e) => {
            error!("Registration failed for email: {}: {}", req.email, e);
            return Err(e); // Return early if registration fails
        }
    }
    
    // Attempt login immediately after successful registration
    match state.auth_service.authenticate(&req.email, &req.password).await {
        Ok(token) => {
            info!("Post-registration login successful for email: {}", req.email);
            Ok(HttpResponse::Created().json(AuthResponse { token }))
        }
        Err(e) => {
            // This case should ideally not happen if registration succeeded and password validation is consistent
            error!("Post-registration login failed unexpectedly for email: {}: {}", req.email, e);
            Err(e) 
        }
    }
}

pub async fn logout(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    // Get token from Authorization header
    let token = req.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(|| Error::Unauthorized("No authorization token provided".into()))?;

    // Invalidate the token
    state.auth_service.invalidate_token(token).await?;
    
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "message": "Successfully logged out"
    })))
} 