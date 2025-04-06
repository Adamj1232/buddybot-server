use std::sync::Arc;
use tokio::sync::mpsc;
use futures::{StreamExt, SinkExt};
use tracing::{error, info};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection as _, Executor, PgPool};
use uuid::Uuid;

use crate::auth::AuthService;
use crate::websocket::{Connection as WebSocketConnection, ConnectionPool};

pub struct WebSocketServer {
    pool: Arc<ConnectionPool>,
    auth_service: Arc<AuthService>,
}

impl WebSocketServer {
    pub fn new(auth_service: Arc<AuthService>) -> Self {
        Self {
            pool: Arc::new(ConnectionPool::new()),
            auth_service,
        }
    }

    pub async fn handle_connection(
        self: Arc<Self>,
        raw_stream: tokio::net::TcpStream,
        addr: std::net::SocketAddr,
    ) {
        info!("New WebSocket connection from: {}", addr);

        let ws_stream = match tokio_tungstenite::accept_async(raw_stream).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("Error during WebSocket handshake: {}", e);
                return;
            }
        };

        let (ws_sink, ws_stream) = ws_stream.split();
        let (tx, rx) = mpsc::unbounded_channel();

        let mut connection = WebSocketConnection::new(
            tx.clone(),
            self.auth_service.clone(),
        );

        // Start connection heartbeat
        connection.start_heartbeat().await;

        // Add connection to pool
        self.pool.add(connection.id(), tx).await;

        let connection_id = connection.id();
        let pool = self.pool.clone();

        // Forward messages from rx to WebSocket
        let send_task = tokio::spawn(async move {
            let mut ws_sink = ws_sink;
            let mut rx = rx;
            
            while let Some(message) = rx.recv().await {
                if let Err(e) = ws_sink.send(message).await {
                    error!("Error sending WebSocket message: {}", e);
                    break;
                }
            }
            
            if let Err(e) = ws_sink.close().await {
                error!("Error closing WebSocket connection: {}", e);
            }
        });

        // Handle incoming WebSocket messages
        let receive_task = tokio::spawn(async move {
            let mut ws_stream = ws_stream;
            
            while let Some(message) = ws_stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Err(e) = connection.handle_message(msg).await {
                            error!("Error handling message: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error receiving WebSocket message: {}", e);
                        break;
                    }
                }
            }
        });

        // Wait for either task to complete
        tokio::select! {
            _ = send_task => {
                info!("Send task completed for connection {}", connection_id);
            }
            _ = receive_task => {
                info!("Receive task completed for connection {}", connection_id);
            }
        }

        // Cleanup connection
        pool.remove(&connection_id).await;
        info!("Connection {} closed", connection_id);
    }

    pub fn pool(&self) -> Arc<ConnectionPool> {
        self.pool.clone()
    }
}

#[allow(dead_code)] // Allow dead code for test helper
async fn setup_test_db_ws() -> (PgPool, String) {
    let db_name = format!("buddybot_test_ws_{}", Uuid::new_v4().to_string());
    let admin_db_url = "postgres://postgres:postgres@localhost:5432/postgres";
    let test_db_url = format!("postgres://postgres:postgres@localhost:5432/{}", db_name);

    // Connect to the default postgres database using the Connection trait
    let mut admin_conn = sqlx::PgConnection::connect(&admin_db_url)
        .await
        .expect("WS: Failed to connect to admin database");

    admin_conn
        .execute(&*format!("DROP DATABASE IF EXISTS \"{}\"", db_name))
        .await
        .expect("WS: Failed to drop test database");

    admin_conn
        .execute(&*format!("CREATE DATABASE \"{}\"", db_name))
        .await
        .expect("WS: Failed to create test database");

    admin_conn.close().await.ok();

    // Connect pool (this is correct)
    let pool = PgPoolOptions::new()
        .connect(&test_db_url)
        .await
        .expect("WS: Failed to connect to test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("WS: Failed to run migrations");

    (pool, db_name)
}

#[allow(dead_code)] // Allow dead code for test helper
async fn cleanup_test_db_ws(db_name: &str) {
    let admin_db_url = "postgres://postgres:postgres@localhost:5432/postgres";
    // Connect to the default postgres database using the Connection trait
    let mut admin_conn = sqlx::PgConnection::connect(&admin_db_url)
        .await
        .expect("WS: Failed to connect to admin database for cleanup");

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
        .expect("WS: Failed to drop test database during cleanup");

    admin_conn.close().await.ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use futures::{StreamExt, SinkExt};
    use tokio::net::TcpListener;
    use tokio::time::sleep;
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use url::Url;
    use serde_json::json;
    use tracing_subscriber;
    use uuid::Uuid;
    use crate::auth::AuthService;
    use crate::db::DbOperations;

    const POLL_INTERVAL: Duration = Duration::from_millis(100);

    #[allow(dead_code)] // Allow dead code for test helper inside test module
    async fn setup_test_db_ws() -> (PgPool, String) {
        let db_name = format!("buddybot_test_ws_{}", Uuid::new_v4().to_string());
        let admin_db_url = "postgres://postgres:postgres@localhost:5432/postgres";
        let test_db_url = format!("postgres://postgres:postgres@localhost:5432/{}", db_name);

        // Connect to the default postgres database using the Connection trait
        let mut admin_conn = sqlx::PgConnection::connect(&admin_db_url)
            .await
            .expect("WS: Failed to connect to admin database");

        admin_conn
            .execute(&*format!("DROP DATABASE IF EXISTS \"{}\"", db_name))
            .await
            .expect("WS: Failed to drop test database");

        admin_conn
            .execute(&*format!("CREATE DATABASE \"{}\"", db_name))
            .await
            .expect("WS: Failed to create test database");

        admin_conn.close().await.ok();

        // Connect pool (this is correct)
        let pool = PgPoolOptions::new()
            .connect(&test_db_url)
            .await
            .expect("WS: Failed to connect to test database");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("WS: Failed to run migrations");

        (pool, db_name)
    }

    #[allow(dead_code)] // Allow dead code for test helper inside test module
    async fn cleanup_test_db_ws(db_name: &str) {
        let admin_db_url = "postgres://postgres:postgres@localhost:5432/postgres";
        // Connect to the default postgres database using the Connection trait
        let mut admin_conn = sqlx::PgConnection::connect(&admin_db_url)
            .await
            .expect("WS: Failed to connect to admin database for cleanup");

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
            .expect("WS: Failed to drop test database during cleanup");

        admin_conn.close().await.ok();
    }

    #[tokio::test]
    async fn test_websocket_server() {
        let _ = tracing_subscriber::fmt::try_init();
        let (pool, db_name) = setup_test_db_ws().await;
        let db_ops = DbOperations::new(Arc::new(pool.clone()));
        let auth_service = Arc::new(AuthService::new(
            db_ops,
            "test_secret".to_string(),
        ));
        
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_url = format!("ws://{}", addr);

        let server = Arc::new(WebSocketServer::new(auth_service));
        let server_clone = server.clone();

        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let server = server_clone.clone();
                tokio::spawn(async move {
                    server.handle_connection(stream, addr).await;
                });
            }
        });

        sleep(POLL_INTERVAL).await;

        // --- Client Connection & Test Logic --- 
        let url = Url::parse(&server_url).unwrap();
        let (ws_stream, _) = connect_async(url.clone()).await.unwrap();
        let (mut write, mut read) = ws_stream.split();

        // Test valid authentication (replace with actual token generation if needed)
        let auth_msg = json!({
            "type": "auth",
            "payload": { "token": "generate_valid_test_token_here" } // Placeholder
        });
        write.send(Message::Text(auth_msg.to_string())).await.unwrap();

        // Wait for auth response (implement wait_for_message if needed)
        // let auth_success = wait_for_message(&mut read, |response| response["type"] == "auth_result").await;
        // assert!(auth_success, "Failed to receive auth response");
        sleep(POLL_INTERVAL * 2).await; // Simple sleep for now

        // Verify connection is tracked (if server exposes this)
        // assert_eq!(server.connection_count().await, 1); // Example

        // Test heartbeat
        let ping_msg = json!({ "type": "ping" });
        write.send(Message::Text(ping_msg.to_string())).await.unwrap();

        // Verify pong response (implement wait_for_message if needed)
        // let pong_received = wait_for_message(&mut read, |response| response["type"] == "pong").await;
        // assert!(pong_received, "Failed to receive pong response");
        sleep(POLL_INTERVAL * 2).await; // Simple sleep for now

        // Test connection close
        write.close().await.unwrap();
        sleep(POLL_INTERVAL * 2).await; // Give time for server to handle close
        
        // Verify connection is removed (if server exposes this)
        // assert_eq!(server.connection_count().await, 0); // Example
        // --- End Client Connection & Test Logic ---

        // Cleanup
        pool.close().await;
        cleanup_test_db_ws(&db_name).await;
    }
} 