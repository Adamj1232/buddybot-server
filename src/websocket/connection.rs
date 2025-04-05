use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use crate::auth::AuthService;
use crate::error::Error;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use std::time::Duration;
use tokio::time::sleep;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(40);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ClientMessage {
    #[serde(rename = "auth")]
    Authenticate { token: String },
    #[serde(rename = "query")]
    Query { text: String },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ServerMessage {
    #[serde(rename = "auth_result")]
    AuthResult { success: bool, error: Option<String> },
    #[serde(rename = "response")]
    Response { text: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
}

pub struct Connection {
    id: Uuid,
    user_id: Option<Uuid>,
    tx: mpsc::UnboundedSender<Message>,
    auth_service: Arc<AuthService>,
    last_heartbeat: Arc<RwLock<std::time::Instant>>,
    authenticated: Arc<RwLock<bool>>,
}

impl Connection {
    pub fn new(
        tx: mpsc::UnboundedSender<Message>,
        auth_service: Arc<AuthService>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id: None,
            tx,
            auth_service,
            last_heartbeat: Arc::new(RwLock::new(std::time::Instant::now())),
            authenticated: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn handle_message(&mut self, msg: Message) -> Result<(), Error> {
        match msg {
            Message::Text(text) => {
                let client_msg: ClientMessage = serde_json::from_str(&text)
                    .map_err(|e| Error::External(format!("Invalid message format: {}", e)))?;

                match client_msg {
                    ClientMessage::Authenticate { token } => {
                        self.handle_auth(token).await?;
                    }
                    ClientMessage::Query { text: query_text } => {
                        if !*self.authenticated.read().await {
                            self.send_error("Not authenticated").await?;
                            return Ok(());
                        }
                        // Handle query - will be implemented in the next phase
                        self.send_message(ServerMessage::Response {
                            text: format!("Query received: {}", query_text),
                        }).await?;
                    }
                    ClientMessage::Ping => {
                        self.handle_ping().await?;
                    }
                    ClientMessage::Pong => {
                        self.handle_pong().await?;
                    }
                }
            }
            Message::Close(_) => {
                info!("Client initiated close for connection {}", self.id);
                return Err(Error::External("Connection closed by client".to_string()));
            }
            Message::Ping(data) => {
                self.tx.send(Message::Pong(data))
                    .map_err(|e| Error::External(format!("Failed to send pong: {}", e)))?;
            }
            Message::Pong(_) => {
                *self.last_heartbeat.write().await = std::time::Instant::now();
            }
            _ => {
                warn!("Received unsupported message type on connection {}", self.id);
            }
        }
        Ok(())
    }

    async fn handle_auth(&mut self, token: String) -> Result<(), Error> {
        match self.auth_service.validate_token(&token).await {
            Ok(user) => {
                self.user_id = Some(user.id);
                *self.authenticated.write().await = true;
                info!("User {} authenticated on connection {}", user.id, self.id);
                self.send_message(ServerMessage::AuthResult {
                    success: true,
                    error: None,
                }).await?;
            }
            Err(e) => {
                error!("Authentication failed for connection {}: {}", self.id, e);
                self.send_message(ServerMessage::AuthResult {
                    success: false,
                    error: Some(e.to_string()),
                }).await?;
            }
        }
        Ok(())
    }

    async fn handle_ping(&self) -> Result<(), Error> {
        self.send_message(ServerMessage::Pong).await
    }

    async fn handle_pong(&self) -> Result<(), Error> {
        *self.last_heartbeat.write().await = std::time::Instant::now();
        Ok(())
    }

    async fn send_message(&self, msg: ServerMessage) -> Result<(), Error> {
        let text = serde_json::to_string(&msg)
            .map_err(|e| Error::External(format!("Failed to serialize message: {}", e)))?;
        
        self.tx.send(Message::Text(text))
            .map_err(|e| Error::External(format!("Failed to send message: {}", e)))?;
        
        Ok(())
    }

    async fn send_error(&self, message: &str) -> Result<(), Error> {
        self.send_message(ServerMessage::Error {
            message: message.to_string(),
        }).await
    }

    pub async fn start_heartbeat(&self) {
        let last_heartbeat = self.last_heartbeat.clone();
        let tx = self.tx.clone();
        let id = self.id;

        tokio::spawn(async move {
            loop {
                sleep(HEARTBEAT_INTERVAL).await;
                
                let elapsed = std::time::Instant::now()
                    .duration_since(*last_heartbeat.read().await);
                
                if elapsed > HEARTBEAT_TIMEOUT {
                    error!("Heartbeat timeout for connection {}", id);
                    break;
                }

                if let Err(e) = tx.send(Message::Ping(vec![])) {
                    error!("Failed to send heartbeat for connection {}: {}", id, e);
                    break;
                }
            }
        });
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn user_id(&self) -> Option<Uuid> {
        self.user_id
    }
} 