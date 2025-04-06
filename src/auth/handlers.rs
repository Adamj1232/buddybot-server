use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use crate::AppState;
use crate::error::Error;

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
    let token = state.auth_service.authenticate(&req.email, &req.password).await?;
    
    Ok(HttpResponse::Ok().json(AuthResponse { token }))
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
    state.auth_service.register(
        &req.email,
        &req.password,
        req.display_name.as_deref(),
    ).await?;
    
    let token = state.auth_service.authenticate(&req.email, &req.password).await?;
    
    Ok(HttpResponse::Created().json(AuthResponse { token }))
} 