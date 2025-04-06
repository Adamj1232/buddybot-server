use sqlx::PgPool;
use uuid::Uuid;
use chrono::{Utc};
use crate::db::models::{User, UserSession};
use crate::error::Error;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Transaction, Postgres};
use std::time::Duration;
use std::sync::Arc;
use sqlx::{Connection, Executor};

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

    pub async fn delete_session(&self, token: &str) -> Result<(), Error> {
        sqlx::query!(
            "DELETE FROM user_sessions WHERE token = $1",
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

#[allow(dead_code)] // Allow dead code for test helper
async fn setup_test_db() -> (PgPool, String) {
    let db_name = format!("buddybot_test_{}", Uuid::new_v4().to_string());
    let admin_db_url = "postgres://postgres:postgres@localhost:5432/postgres";
    let test_db_url = format!("postgres://postgres:postgres@localhost:5432/{}", db_name);

    let mut admin_conn = sqlx::PgConnection::connect(&admin_db_url)
        .await
        .expect("Failed to connect to admin database");

    admin_conn
        .execute(&*format!("DROP DATABASE IF EXISTS \"{}\"", db_name))
        .await
        .expect("Failed to drop test database");

    admin_conn
        .execute(&*format!("CREATE DATABASE \"{}\"", db_name))
        .await
        .expect("Failed to create test database");

    admin_conn.close().await.ok();

    let pool = PgPoolOptions::new()
        .connect(&test_db_url)
        .await
        .expect("Failed to connect to test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    (pool, db_name)
}

#[allow(dead_code)] // Allow dead code for test helper
async fn cleanup_test_db(db_name: &str) {
    let admin_db_url = "postgres://postgres:postgres@localhost:5432/postgres";
    let mut admin_conn = sqlx::PgConnection::connect(&admin_db_url)
        .await
        .expect("Failed to connect to admin database for cleanup");

    admin_conn
        .execute(&*format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
            db_name
        ))
        .await
        .ok();
    admin_conn
        .execute(&*format!("DROP DATABASE IF EXISTS \"{}\"", db_name))
        .await
        .expect("Failed to drop test database during cleanup");

    admin_conn.close().await.ok();
}

#[tokio::test]
async fn test_transaction_rollback() {
    let (pool, db_name) = setup_test_db().await;
    let db = DbOperations::new(Arc::new(pool));
    let mut transaction = db.begin_transaction().await.unwrap();
    
    let user = User::new(
        "test@example.com".to_string(),
        Some("Test User".to_string()),
    );

    let created_user = sqlx::query_as!(
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
    .fetch_one(&mut *transaction)
    .await
    .unwrap();

    let found_user = sqlx::query_as!(
        User,
        "SELECT id, email, display_name, created_at, updated_at, last_login, is_active, rate_limit_tier FROM users WHERE id = $1",
        created_user.id
    )
    .fetch_optional(&mut *transaction)
    .await
    .unwrap();

    assert!(found_user.is_some());

    transaction.rollback().await.unwrap();

    let found_user = sqlx::query_as!(
        User,
        "SELECT id, email, display_name, created_at, updated_at, last_login, is_active, rate_limit_tier FROM users WHERE id = $1",
        created_user.id
    )
    .fetch_optional(db.pool.as_ref())
    .await
    .unwrap();

    assert!(found_user.is_none());

    db.pool.close().await;
    cleanup_test_db(&db_name).await;
}

#[tokio::test]
async fn test_pool_status() {
    let (pool, db_name) = setup_test_db().await;
    let db = DbOperations::new(Arc::new(pool));
    let status = db.get_pool_status().await.unwrap();
    
    assert!(status.total_connections <= 5, "Total connections should not exceed max");
    assert!(status.idle_connections <= status.total_connections, "Idle connections should not exceed total");
    assert!(status.active_connections <= status.total_connections, "Active connections should not exceed total");
    assert_eq!(status.active_connections + status.idle_connections, status.total_connections, "Active + Idle should equal Total");

    db.pool.close().await;
    cleanup_test_db(&db_name).await;
} 