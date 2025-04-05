//! Authentication module for BuddyBot server
//! 
//! This module handles user authentication, token management,
//! and session handling.

// Re-export public interfaces
// Will be implemented in Phase 2

mod service;
mod rate_limit;

pub use service::{AuthService, Claims};
pub use rate_limit::{RateLimiter, RateLimitConfig};
