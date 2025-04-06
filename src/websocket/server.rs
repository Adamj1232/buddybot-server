use std::sync::Arc;
use tokio::sync::mpsc;
use futures::{StreamExt, SinkExt};
use tracing::{error, info};

use crate::auth::AuthService;
use crate::websocket::{Connection, ConnectionPool};
use sqlx::PgPool;

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

        let mut connection = Connection::new(
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

impl Drop for WebSocketServer {
    fn drop(&mut self) {
        // Clean up test database in a blocking task
        if std::env::var("TEST").is_ok() {
            let admin_url = "postgres://postgres:postgres@localhost:5432/postgres".to_string();
            tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    if let Ok(pool) = PgPool::connect(&admin_url).await {
                        let _ = sqlx::query("DROP DATABASE IF EXISTS buddybot_test;")
                            .execute(&pool)
                            .await;
                    }
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;
    use futures::{StreamExt, SinkExt};
    use tokio::net::TcpListener;
    use tokio::time::sleep;
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use url::Url;
    use sqlx::PgPool;
    use serde_json::json;
    use tracing_subscriber;

    use crate::auth::AuthService;
    use crate::db::DbOperations;
    use super::WebSocketServer;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);
    const POLL_INTERVAL: Duration = Duration::from_millis(100);

    async fn setup_test_server() -> (Arc<WebSocketServer>, String) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_url = format!("ws://{}", addr);

        // Connect to default postgres database first
        let admin_url = "postgres://postgres:postgres@localhost:5432/postgres".to_string();
        let admin_pool = PgPool::connect(&admin_url).await.unwrap();

        // Drop test database if it exists
        let _ = sqlx::query("DROP DATABASE IF EXISTS buddybot_test;")
            .execute(&admin_pool)
            .await;

        // Create test database
        let create_db_query = "CREATE DATABASE buddybot_test;";
        sqlx::query(create_db_query)
            .execute(&admin_pool)
            .await
            .expect("Failed to create test database");

        // Connect to test database and run migrations
        let database_url = "postgres://postgres:postgres@localhost:5432/buddybot_test".to_string();
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        sqlx::migrate!()
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        let pool = Arc::new(pool);
        let db = DbOperations::new(pool);
        
        let auth_service = Arc::new(AuthService::new(
            db,
            "test_secret".to_string(),
        ));

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
        (server, server_url)
    }

    async fn wait_for_condition<F>(mut condition: F, timeout: Duration) -> bool 
    where
        F: FnMut() -> bool
    {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if condition() {
                return true;
            }
            sleep(POLL_INTERVAL).await;
        }
        false
    }

    async fn wait_for_message<S, F>(read: &mut futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<S>>, predicate: F) -> bool
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        F: Fn(&serde_json::Value) -> bool
    {
        let start = std::time::Instant::now();
        while start.elapsed() < TEST_TIMEOUT {
            if let Some(Ok(msg)) = read.next().await {
                match msg {
                    Message::Text(text) => {
                        if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                            if predicate(&response) {
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            sleep(POLL_INTERVAL).await;
        }
        false
    }

    #[tokio::test]
    async fn test_websocket_server() {
        let _ = tracing_subscriber::fmt::try_init();
        let (server, url) = setup_test_server().await;
        
        // Connect client
        let url = Url::parse(&url).unwrap();
        let (ws_stream, _) = connect_async(url.clone()).await.unwrap();
        let (mut write, mut read) = ws_stream.split();

        // Test valid authentication first
        let auth_msg = json!({
            "type": "auth",
            "payload": {
                "token": "test_token"
            }
        });
        write.send(Message::Text(auth_msg.to_string())).await.unwrap();

        // Wait for auth response
        let auth_success = wait_for_message(&mut read, |response| {
            response["type"] == "auth_result"
        }).await;
        assert!(auth_success, "Failed to receive auth response");

        // Verify connection is in pool
        let pool_verified = wait_for_condition(|| {
            let count = futures::executor::block_on(server.pool().connection_count());
            count == 1
        }, TEST_TIMEOUT).await;
        assert!(pool_verified, "Connection not found in pool");

        // Test heartbeat
        let ping_msg = json!({
            "type": "ping"
        });
        write.send(Message::Text(ping_msg.to_string())).await.unwrap();

        // Verify ping response
        let pong_received = wait_for_message(&mut read, |response| {
            response["type"] == "pong"
        }).await;
        assert!(pong_received, "Failed to receive pong response");

        // Test connection close
        write.send(Message::Close(None)).await.unwrap();
        sleep(POLL_INTERVAL).await;
        
        // Verify connection is removed from pool
        let pool_empty = wait_for_condition(|| {
            let count = futures::executor::block_on(server.pool().connection_count());
            count == 0
        }, TEST_TIMEOUT).await;
        assert!(pool_empty, "Connection not removed from pool");

        // Test reconnection
        let (ws_stream, _) = connect_async(url).await.unwrap();
        let (mut write, _) = ws_stream.split();
        write.send(Message::Text(auth_msg.to_string())).await.unwrap();
        
        // Verify new connection is added to pool
        let pool_reconnected = wait_for_condition(|| {
            let count = futures::executor::block_on(server.pool().connection_count());
            count == 1
        }, TEST_TIMEOUT).await;
        assert!(pool_reconnected, "Failed to reconnect");

        // Clean up
        write.send(Message::Close(None)).await.unwrap();
        sleep(POLL_INTERVAL).await;
    }
} 