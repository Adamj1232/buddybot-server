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
}

impl AuthService {
    pub fn new(
        db: DbOperations,
        jwt_secret: String,
    ) -> Self {
        Self {
            db,
            jwt_secret,
        }
    }

    pub async fn authenticate(&self, email: &str, password: &str) -> Result<String, Error> {
        let user = self.db.get_user_by_email(email).await?
            .ok_or_else(|| Error::Unauthorized("Invalid credentials".into()))?;

        // TODO: Implement proper password validation
        if password.is_empty() {
            return Err(Error::Unauthorized("Invalid credentials".into()));
        }

        let token = self.generate_token(&user.id.to_string())?;

        let session = UserSession::new(user.id, token.clone(), 24);
        self.db.create_session(&session).await?;

        Ok(token)
    }

    pub async fn validate_token(&self, token: &str) -> Result<User, Error> {
        let session = self.db.get_session_by_token(token).await?
            .ok_or_else(|| Error::Unauthorized("Invalid session".into()))?;

        if session.is_expired() {
            return Err(Error::Unauthorized("Session expired".into()));
        }

        let claims = self.decode_token(token)?;

        let user = self.db.get_user_by_id(Uuid::parse_str(&claims.sub)?).await?
            .ok_or_else(|| Error::Unauthorized("User not found".into()))?;

        self.db.update_session_activity(token).await?;

        Ok(user)
    }

    pub async fn register(
        &self,
        email: &str,
        password: &str,
        display_name: Option<&str>,
    ) -> Result<User, Error> {
        // TODO: Add proper password hashing
        if password.is_empty() {
            return Err(Error::Unauthorized("Password cannot be empty".into()));
        }

        let user = User::new(
            email.to_string(),
            display_name.map(|s| s.to_string()),
        );

        let user = self.db.create_user(&user).await?;
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

    pub async fn invalidate_token(&self, token: &str) -> Result<(), Error> {
        self.db.delete_session(token).await?;
        Ok(())
    }
} 