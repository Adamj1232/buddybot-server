use sqlx::PgPool;
use uuid::Uuid;
use chrono::{Utc};
use crate::db::models::{User, UserSession};
use crate::error::Error;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Transaction, Postgres};
use std::time::Duration;
use std::sync::Arc;

pub struct DbOperations {
    pool: Arc<PgPool>,
}

impl DbOperations {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn new_with_options(
        url: &str,
        max_connections: u32,
        acquire_timeout: Duration,
    ) -> Result<Self, Error> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(acquire_timeout)
            .connect(url)
            .await?;

        Ok(Self { pool: Arc::new(pool) })
    }

    pub async fn get_pool_status(&self) -> Result<DbPoolStatus, Error> {
        let size = self.pool.size() as u32;
        let idle = self.pool.num_idle() as u32;
        let active = size - idle;

        Ok(DbPoolStatus {
            total_connections: size,
            active_connections: active,
            idle_connections: idle,
        })
    }

    pub async fn begin_transaction(&self) -> Result<Transaction<'_, Postgres>, Error> {
        Ok(self.pool.as_ref().begin().await?)
    }

    pub async fn create_user_with_transaction<'a>(
        &self,
        user: &User,
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<User, Error> {
        let user = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (id, email, display_name, created_at, updated_at, is_active, rate_limit_tier)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, email, display_name, created_at, updated_at, last_login, is_active, rate_limit_tier
            "#,
            user.id,
            user.email,
            user.display_name,
            user.created_at,
            user.updated_at,
            user.is_active,
            user.rate_limit_tier
        )
        .fetch_one(&mut **transaction)
        .await?;

        Ok(user)
    }

    pub async fn create_user(&self, user: &User) -> Result<User, Error> {
        let mut transaction = self.begin_transaction().await?;
        
        let result = self.create_user_with_transaction(user, &mut transaction).await;
        
        match result {
            Ok(user) => {
                transaction.commit().await?;
                Ok(user)
            }
            Err(e) => {
                transaction.rollback().await?;
                Err(e)
            }
        }
    }

    pub async fn get_user_by_id(&self, id: Uuid) -> Result<Option<User>, Error> {
        let user = sqlx::query_as!(
            User,
            "SELECT id, email, display_name, created_at, updated_at, last_login, is_active, rate_limit_tier FROM users WHERE id = $1",
            id
        )
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(user)
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, Error> {
        let user = sqlx::query_as!(
            User,
            "SELECT id, email, display_name, created_at, updated_at, last_login, is_active, rate_limit_tier FROM users WHERE email = $1",
            email
        )
        .fetch_optional(self.pool.as_ref())
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
        .fetch_one(self.pool.as_ref())
        .await?;

        Ok(session)
    }

    pub async fn get_session_by_token(&self, token: &str) -> Result<Option<UserSession>, Error> {
        let session = sqlx::query_as!(
            UserSession,
            "SELECT * FROM user_sessions WHERE token = $1",
            token
        )
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(session)
    }

    pub async fn update_session_activity(&self, token: &str) -> Result<(), Error> {
        sqlx::query!(
            "UPDATE user_sessions SET last_activity = $1 WHERE token = $2",
            Utc::now(),
            token
        )
        .execute(self.pool.as_ref())
        .await?;

        Ok(())
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, Error> {
        let mut transaction = self.begin_transaction().await?;
        
        let result = sqlx::query!(
            "DELETE FROM user_sessions WHERE expires_at < $1",
            Utc::now()
        )
        .execute(&mut *transaction)
        .await;

        match result {
            Ok(result) => {
                transaction.commit().await?;
                Ok(result.rows_affected())
            }
            Err(e) => {
                transaction.rollback().await?;
                Err(e.into())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct DbPoolStatus {
    pub total_connections: u32,
    pub active_connections: u32,
    pub idle_connections: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::User;

    #[tokio::test]
    async fn test_transaction_rollback() {
        let db = DbOperations::new_with_options(
            "postgres://postgres:postgres@localhost:5432/buddybot_test",
            5,
            Duration::from_secs(5),
        ).await.unwrap();

        let mut transaction = db.begin_transaction().await.unwrap();
        
        // Create a test user
        let user = User::new(
            "test@example.com".to_string(),
            Some("Test User".to_string()),
        );

        // Insert user in transaction
        let created_user = db.create_user_with_transaction(&user, &mut transaction)
            .await
            .unwrap();

        // Verify user exists in transaction
        let found_user = sqlx::query_as!(
            User,
            "SELECT id, email, display_name, created_at, updated_at, last_login, is_active, rate_limit_tier FROM users WHERE id = $1",
            created_user.id
        )
        .fetch_optional(&mut *transaction)
        .await
        .unwrap();

        assert!(found_user.is_some());

        // Rollback transaction
        transaction.rollback().await.unwrap();

        // Verify user doesn't exist after rollback
        let found_user = sqlx::query_as!(
            User,
            "SELECT id, email, display_name, created_at, updated_at, last_login, is_active, rate_limit_tier FROM users WHERE id = $1",
            created_user.id
        )
        .fetch_optional(db.pool.as_ref())
        .await
        .unwrap();

        assert!(found_user.is_none());
    }

    #[tokio::test]
    async fn test_pool_status() {
        let db = DbOperations::new_with_options(
            "postgres://postgres:postgres@localhost:5432/buddybot_test",
            5,
            Duration::from_secs(5),
        ).await.unwrap();

        let status = db.get_pool_status().await.unwrap();
        assert_eq!(status.total_connections, 5);
        assert!(status.idle_connections <= 5);
        assert!(status.active_connections <= 5);
    }
} 