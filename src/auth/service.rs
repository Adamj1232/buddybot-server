use crate::db::operations::DbOperations;
use crate::db::models::{User, UserSession};
use crate::error::Error;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // User ID
    pub exp: i64,     // Expiration time
    pub iat: i64,     // Issued at
}

pub struct AuthService {
    db: DbOperations,
    jwt_secret: String,
    uauth_client_id: String,
    uauth_client_secret: String,
}

impl AuthService {
    pub fn new(
        db: DbOperations,
        jwt_secret: String,
        uauth_client_id: String,
        uauth_client_secret: String,
    ) -> Self {
        Self {
            db,
            jwt_secret,
            uauth_client_id,
            uauth_client_secret,
        }
    }

    pub async fn authenticate_uauth(&self, code: &str) -> Result<String, Error> {
        // Exchange code for uAuth token
        let token = self.exchange_code_for_token(code).await?;
        
        // Get user info from uAuth
        let uauth_user = self.get_uauth_user_info(&token).await?;
        
        // Find or create user
        let user = match self.db.get_user_by_uauth_id(&uauth_user.id).await? {
            Some(user) => user,
            None => {
                let new_user = User::new(
                    uauth_user.email,
                    uauth_user.id,
                    Some(uauth_user.name),
                );
                self.db.create_user(&new_user).await?
            }
        };

        // Generate JWT token
        let token = self.generate_token(&user.id.to_string())?;

        // Create session
        let session = UserSession::new(user.id, token.clone(), 24);
        self.db.create_session(&session).await?;

        Ok(token)
    }

    pub async fn validate_token(&self, token: &str) -> Result<User, Error> {
        // First check if session exists and is not expired
        let session = self.db.get_session_by_token(token).await?
            .ok_or_else(|| Error::Unauthorized("Invalid session".into()))?;

        if session.is_expired() {
            return Err(Error::Unauthorized("Session expired".into()));
        }

        // Validate JWT
        let claims = self.decode_token(token)?;

        // Get user
        let user = self.db.get_user_by_id(Uuid::parse_str(&claims.sub)?).await?
            .ok_or_else(|| Error::Unauthorized("User not found".into()))?;

        // Update session activity
        self.db.update_session_activity(token).await?;

        Ok(user)
    }

    fn generate_token(&self, user_id: &str) -> Result<String, Error> {
        let now = Utc::now();
        let exp = (now + Duration::hours(24)).timestamp();
        let claims = Claims {
            sub: user_id.to_string(),
            exp,
            iat: now.timestamp(),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )?;

        Ok(token)
    }

    fn decode_token(&self, token: &str) -> Result<Claims, Error> {
        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )?;

        Ok(claims.claims)
    }

    async fn exchange_code_for_token(&self, code: &str) -> Result<String, Error> {
        let client = reqwest::Client::new();
        let res = client.post("https://uauth.io/oauth/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.uauth_client_id),
                ("client_secret", &self.uauth_client_secret),
            ])
            .send()
            .await?;

        let token_response: serde_json::Value = res.json().await?;
        let access_token = token_response["access_token"]
            .as_str()
            .ok_or_else(|| Error::External("Invalid token response".into()))?;

        Ok(access_token.to_string())
    }

    async fn get_uauth_user_info(&self, token: &str) -> Result<UAuthUser, Error> {
        let client = reqwest::Client::new();
        let res = client.get("https://uauth.io/api/v1/user")
            .bearer_auth(token)
            .send()
            .await?;

        let user: UAuthUser = res.json().await?;
        Ok(user)
    }
}

#[derive(Debug, Deserialize)]
struct UAuthUser {
    id: String,
    email: String,
    name: String,
} 