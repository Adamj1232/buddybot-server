//! WebSocket module for BuddyBot server
//! 
//! This module handles WebSocket connections, message processing,
//! and client session management.

// Re-export public interfaces
// Will be implemented in Phase 2

mod connection;
mod pool;
mod server;

pub use connection::{Connection, ClientMessage, ServerMessage};
pub use pool::ConnectionPool;
pub use server::WebSocketServer;
