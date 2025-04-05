use sqlx::PgPool;
use uuid::Uuid;
use chrono::{Utc};
use crate::db::models::{User, UserSession};
use crate::error::Error;

pub struct DbOperations {
    pool: PgPool,
}

impl DbOperations {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_user(&self, user: &User) -> Result<User, Error> {
        let user = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (id, email, uauth_id, display_name, created_at, updated_at, is_active, rate_limit_tier)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
            user.id,
            user.email,
            user.uauth_id,
            user.display_name,
            user.created_at,
            user.updated_at,
            user.is_active,
            user.rate_limit_tier
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn get_user_by_id(&self, id: Uuid) -> Result<Option<User>, Error> {
        let user = sqlx::query_as!(
            User,
            "SELECT * FROM users WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn get_user_by_uauth_id(&self, uauth_id: &str) -> Result<Option<User>, Error> {
        let user = sqlx::query_as!(
            User,
            "SELECT * FROM users WHERE uauth_id = $1",
            uauth_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(user)
    }

    pub async fn create_session(&self, session: &UserSession) -> Result<UserSession, Error> {
        let session = sqlx::query_as!(
            UserSession,
            r#"
            INSERT INTO user_sessions (user_id, token, expires_at, created_at, last_activity)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            session.user_id,
            session.token,
            session.expires_at,
            session.created_at,
            session.last_activity
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(session)
    }

    pub async fn get_session_by_token(&self, token: &str) -> Result<Option<UserSession>, Error> {
        let session = sqlx::query_as!(
            UserSession,
            "SELECT * FROM user_sessions WHERE token = $1",
            token
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(session)
    }

    pub async fn update_session_activity(&self, token: &str) -> Result<(), Error> {
        sqlx::query!(
            "UPDATE user_sessions SET last_activity = $1 WHERE token = $2",
            Utc::now(),
            token
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_expired_sessions(&self) -> Result<u64, Error> {
        let result = sqlx::query!(
            "DELETE FROM user_sessions WHERE expires_at < $1",
            Utc::now()
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
} 