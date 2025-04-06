use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub window_size: Duration,
    pub limits: HashMap<String, u32>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        let mut limits = HashMap::new();
        limits.insert("standard".to_string(), 100);  // 100 requests per window
        limits.insert("premium".to_string(), 500);   // 500 requests per window
        
        Self {
            window_size: Duration::minutes(1),
            limits,
        }
    }
}

#[derive(Debug)]
struct RequestWindow {
    timestamps: Vec<DateTime<Utc>>,
}

impl RequestWindow {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
        }
    }

    fn cleanup_old_requests(&mut self, window_size: Duration) {
        let cutoff = Utc::now() - window_size;
        self.timestamps.retain(|ts| *ts > cutoff);
    }

    fn add_request(&mut self) {
        self.timestamps.push(Utc::now());
    }

    fn request_count(&self) -> usize {
        self.timestamps.len()
    }
}

pub struct RateLimiter {
    windows: Arc<RwLock<HashMap<Uuid, RequestWindow>>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    pub async fn check_rate_limit(&self, user_id: Uuid, tier: &str) -> bool {
        let mut windows = self.windows.write().await;
        
        // Get or create window for user
        let window = windows.entry(user_id).or_insert_with(RequestWindow::new);
        
        // Cleanup old requests
        window.cleanup_old_requests(self.config.window_size);
        
        // Get limit for user's tier
        let limit = self.config.limits.get(tier)
            .unwrap_or_else(|| self.config.limits.get("standard").unwrap());
        
        // Check if under limit
        if window.request_count() < *limit as usize {
            window.add_request();
            true
        } else {
            false
        }
    }

    pub async fn cleanup(&self) {
        let mut windows = self.windows.write().await;
        
        // Remove windows with no recent requests
        windows.retain(|_, window| {
            window.cleanup_old_requests(self.config.window_size);
            !window.timestamps.is_empty()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration as TokioDuration};

    #[tokio::test]
    async fn test_rate_limiter() {
        let mut config = RateLimitConfig::default();
        // Use a shorter window for testing
        config.window_size = Duration::seconds(1);
        let limiter = RateLimiter::new(config);
        let user_id = Uuid::new_v4();

        // Should allow requests up to limit
        for _ in 0..100 {
            assert!(limiter.check_rate_limit(user_id, "standard").await);
        }

        // Should deny requests over limit
        assert!(!limiter.check_rate_limit(user_id, "standard").await);

        // Wait for window to pass
        sleep(TokioDuration::from_millis(1100)).await;

        // Should allow requests again
        assert!(limiter.check_rate_limit(user_id, "standard").await);
    }
} 