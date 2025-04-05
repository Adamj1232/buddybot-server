//! Database module for BuddyBot server
//! 
//! This module handles database connections, migrations,
//! and data access layer operations.

// Re-export public interfaces
// Will be implemented in Phase 2

pub mod models;
pub mod operations;

pub use models::{User, UserSession};
pub use operations::DbOperations;
