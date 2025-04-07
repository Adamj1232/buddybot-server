use actix_web::{web, App, HttpServer, HttpResponse, Error, HttpRequest};
use actix_cors::Cors;
use actix::prelude::*;
use actix_web_actors::ws;
use buddybot_server::{AppState, Settings, AppError};
use buddybot_server::auth::handlers::{login, register, logout};
use buddybot_server::websocket::{ClientMessage, ServerMessage};
use dotenv::dotenv;
use std::net::TcpListener;
use tracing::{info, error, warn, Level};
use tracing_subscriber::FmtSubscriber;
use std::time::Duration;
use std::sync::Arc;
use uuid::Uuid;

/// Health check endpoint handler
/// Returns a JSON response with server status and timestamp
async fn health_check(state: web::Data<AppState>) -> HttpResponse {
    let instances = state.scaling.get_active_instances().await;

    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "instances": instances,
    }))
}

/// WebSocket connection handler
/// This upgrades the HTTP connection to a WebSocket connection
async fn websocket_route(
    req: HttpRequest,
    stream: web::Payload,
    app_data: web::Data<AppState>,
) -> std::result::Result<HttpResponse, Error> {
    let peer_addr = req.peer_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    
    info!("New WebSocket connection request from: {}", peer_addr);
    
    // Create WebSocket actor and start it
    ws::start(
        WebSocketSession::new(app_data.ws_server.clone(), peer_addr),
        &req,
        stream,
    )
}

/// WebSocket session actor that handles WebSocket connections
struct WebSocketSession {
    ws_server: Arc<buddybot_server::websocket::WebSocketServer>,
    peer_addr: String,
    id: Uuid,
    authenticated: bool,
}

impl WebSocketSession {
    fn new(ws_server: Arc<buddybot_server::websocket::WebSocketServer>, peer_addr: String) -> Self {
        Self { 
            ws_server,
            peer_addr,
            id: Uuid::new_v4(),
            authenticated: false,
        }
    }

    /// Process an incoming message and generate a response
    fn handle_websocket_message(&mut self, text: String, ctx: &mut <Self as Actor>::Context) {
        // Log the received message
        info!("Received message from {}: {}", self.peer_addr, text);

        // Parse the message as a ClientMessage
        match serde_json::from_str::<ClientMessage>(&text) {
            Ok(client_msg) => {
                match client_msg {
                    ClientMessage::Authenticate { token } => {
                        info!("Authentication attempt from {}", self.peer_addr);
                        // Forward to WebSocketServer for authentication
                        Self::handle_auth_result(self, ctx, token);
                    },
                    ClientMessage::Query { text } => {
                        if !self.authenticated {
                            warn!("Unauthenticated query attempt from {}", self.peer_addr);
                            self.send_error(ctx, "Not authenticated");
                            return;
                        }
                        
                        info!("Query from {}: {}", self.peer_addr, text);
                        // Echo back the message for now
                        // In a real implementation, this would process the query and generate a response
                        self.send_response(ctx, &format!("Echo: {}", text));
                    },
                    ClientMessage::Ping => {
                        // Respond with a pong message
                        self.send_server_message(ctx, ServerMessage::Pong);
                    },
                    ClientMessage::Pong => {
                        // Client responded to our ping, update heartbeat timestamp
                        // This would typically update a last_heartbeat field
                    },
                }
            },
            Err(e) => {
                error!("Failed to parse message from {}: {}", self.peer_addr, e);
                self.send_error(ctx, &format!("Invalid message format: {}", e));
            }
        }
    }

    /// Handle authentication result
    fn handle_auth_result(&mut self, ctx: &mut <Self as Actor>::Context, token: String) {
        // In a real implementation, this would validate the token with your authentication service
        // For the purpose of this example, we'll simply accept any token
        if !token.is_empty() {
            self.authenticated = true;
            info!("Authentication successful for {}", self.peer_addr);
            self.send_server_message(ctx, ServerMessage::AuthResult { 
                success: true, 
                error: None 
            });
        } else {
            self.authenticated = false;
            warn!("Authentication failed for {}", self.peer_addr);
            self.send_server_message(ctx, ServerMessage::AuthResult { 
                success: false, 
                error: Some("Invalid token".to_string()) 
            });
        }
    }

    /// Send a server message to the client
    fn send_server_message(&self, ctx: &mut <Self as Actor>::Context, msg: ServerMessage) {
        match serde_json::to_string(&msg) {
            Ok(json_str) => {
                ctx.text(json_str);
            },
            Err(e) => {
                error!("Failed to serialize server message: {}", e);
            }
        }
    }

    /// Send an error message to the client
    fn send_error(&self, ctx: &mut <Self as Actor>::Context, message: &str) {
        self.send_server_message(ctx, ServerMessage::Error { 
            message: message.to_string() 
        });
    }

    /// Send a response message to the client
    fn send_response(&self, ctx: &mut <Self as Actor>::Context, text: &str) {
        self.send_server_message(ctx, ServerMessage::Response { 
            text: text.to_string() 
        });
    }

    /// Start the heartbeat process
    fn start_heartbeat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Duration::from_secs(30), |act, ctx| {
            // Send a ping message to the client
            act.send_server_message(ctx, ServerMessage::Ping);
        });
    }
}

impl Actor for WebSocketSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("WebSocket connection established with {} (id: {})", self.peer_addr, self.id);
        
        // Start heartbeat
        self.start_heartbeat(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("WebSocket connection closed with {} (id: {})", self.peer_addr, self.id);
    }
}

/// Implement the StreamHandler trait to process WebSocket messages
impl StreamHandler<std::result::Result<ws::Message, ws::ProtocolError>> for WebSocketSession {
    fn handle(&mut self, msg: std::result::Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                info!("Received ping from {}", self.peer_addr);
                ctx.pong(&msg);
            }
            Ok(ws::Message::Text(text)) => {
                self.handle_websocket_message(text.to_string(), ctx);
            }
            Ok(ws::Message::Binary(bin)) => {
                info!("Received binary message from {} of {} bytes", self.peer_addr, bin.len());
                // Binary messages are not supported in this implementation
                self.send_error(ctx, "Binary messages are not supported");
            }
            Ok(ws::Message::Close(reason)) => {
                info!("WebSocket closed from {}: {:?}", self.peer_addr, reason);
                ctx.close(reason);
            }
            Ok(_) => {
                // Other message types can be handled here if needed
            }
            Err(e) => {
                error!("Error handling WebSocket message from {}: {}", self.peer_addr, e);
                ctx.stop();
            }
        }
    }
}

#[actix_web::main]
async fn main() -> buddybot_server::Result<()> {
    // Load environment variables
    dotenv().ok();
    
    // Initialize logging
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .pretty()
        .init();
    
    // Load configuration
    let config = Settings::new()?;
    info!("Configuration loaded successfully");
    
    info!("Starting server at {}:{}", config.server.host, config.server.port);
    
    // Initialize application state
    let state = AppState::new(config.clone()).await?;
    let state = web::Data::new(state);

    // Start instance management
    let scaling_state = state.clone();
    tokio::spawn(async move {
        loop {
            // Check scaling needs
            if let Some(action) = scaling_state.scaling.check_scaling_needs().await {
                info!("Scaling action required: {:?}", action);
                // Implement scaling action here
            }

            // Cleanup inactive instances
            scaling_state.scaling.cleanup_inactive_instances().await;

            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
    
    // Create and bind TCP listener
    let listener = TcpListener::bind(format!("{}:{}", config.server.host, config.server.port))?;
    
    info!("WebSocket server initialized and ready to accept connections at ws://{}:{}/ws", config.server.host, config.server.port);
    
    // Start HTTP server
    HttpServer::new(move || {
        let cors = if config.cors.enabled {
            let cors_config = Cors::default();
            
            // Apply specific CORS rules based on configuration
            let cors_config = if config.cors.allow_any_origin {
                cors_config
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .expose_any_header()
                    .supports_credentials()
            } else {
                // More restrictive CORS for production use
                cors_config
                    .allowed_origin("https://your-production-frontend.com")
                    .allowed_origin("http://localhost:8080")
                    .allowed_origin("http://127.0.0.1:8080")
                    .allowed_methods(vec!["GET", "POST"])
                    .allowed_headers(vec!["Authorization", "Content-Type"])
                    .supports_credentials()
            };
            
            // Set max age
            cors_config.max_age(config.cors.max_age as usize)
        } else {
            // CORS disabled - use most restrictive settings
            Cors::default()
        };

        App::new()
            .wrap(cors)
            .app_data(state.clone())
            .route("/health", web::get().to(health_check))
            .route("/auth/login", web::post().to(login))
            .route("/auth/register", web::post().to(register))
            .route("/auth/logout", web::post().to(logout))
            .route("/ws", web::get().to(websocket_route))  // Add WebSocket route
    })
    .listen(listener)?
    .workers(config.server.workers as usize)
    .run()
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;

    Ok(())
}