//! Scaling module for BuddyBot server
//! 
//! This module handles load balancing, auto-scaling,
//! and resource management across instances.

// Re-export public interfaces
// Will be implemented in Phase 2

use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub connection_count: u64,
    pub active_users: u64,
    pub request_rate: f64,
    pub error_rate: f64,
    pub response_time_p95: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    pub cpu_threshold: f32,
    pub memory_threshold: f32,
    pub connection_threshold: u64,
    pub scale_up_factor: f32,
    pub scale_down_factor: f32,
    pub cooldown_period: i64,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            cpu_threshold: 70.0,      // 70% CPU usage
            memory_threshold: 80.0,    // 80% memory usage
            connection_threshold: 1000, // 1000 connections
            scale_up_factor: 1.5,      // Increase capacity by 50%
            scale_down_factor: 0.5,    // Decrease capacity by 50%
            cooldown_period: 300,      // 5 minutes cooldown
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub id: Uuid,
    pub host: String,
    pub port: u16,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub metrics: Option<SystemMetrics>,
}

pub struct ScalingManager {
    config: Arc<RwLock<ScalingConfig>>,
    instances: Arc<RwLock<HashMap<Uuid, InstanceInfo>>>,
    last_scaling_action: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl ScalingManager {
    pub fn new(config: ScalingConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            instances: Arc::new(RwLock::new(HashMap::new())),
            last_scaling_action: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn register_instance(&self, host: String, port: u16) -> Uuid {
        let instance_id = Uuid::new_v4();
        let now = Utc::now();
        
        let instance = InstanceInfo {
            id: instance_id,
            host,
            port,
            started_at: now,
            last_heartbeat: now,
            metrics: None,
        };

        self.instances.write().await.insert(instance_id, instance);
        info!("Registered new instance: {}", instance_id);
        
        instance_id
    }

    pub async fn update_instance_metrics(&self, instance_id: Uuid, metrics: SystemMetrics) -> Result<(), String> {
        let mut instances = self.instances.write().await;
        
        if let Some(instance) = instances.get_mut(&instance_id) {
            instance.metrics = Some(metrics);
            instance.last_heartbeat = Utc::now();
            Ok(())
        } else {
            Err("Instance not found".to_string())
        }
    }

    pub async fn check_scaling_needs(&self) -> Option<ScalingAction> {
        let config = self.config.read().await;
        let instances = self.instances.read().await;
        let last_action = self.last_scaling_action.read().await;

        // Check cooldown period
        if let Some(last_time) = *last_action {
            if (Utc::now() - last_time).num_seconds() < config.cooldown_period {
                return None;
            }
        }

        // Calculate aggregate metrics
        let mut total_cpu = 0.0;
        let mut total_memory = 0.0;
        let mut total_connections = 0;
        let mut active_instances = 0;

        for instance in instances.values() {
            if let Some(metrics) = &instance.metrics {
                total_cpu += metrics.cpu_usage;
                total_memory += (metrics.memory_used as f32 / metrics.memory_total as f32) * 100.0;
                total_connections += metrics.connection_count;
                active_instances += 1;
            }
        }

        if active_instances == 0 {
            return None;
        }

        let avg_cpu = total_cpu / active_instances as f32;
        let avg_memory = total_memory / active_instances as f32;
        let avg_connections = total_connections / active_instances;

        // Determine if scaling is needed
        if avg_cpu > config.cpu_threshold || 
           avg_memory > config.memory_threshold || 
           avg_connections > config.connection_threshold {
            Some(ScalingAction::ScaleUp(config.scale_up_factor))
        } else if avg_cpu < config.cpu_threshold * 0.5 && 
                  avg_memory < config.memory_threshold * 0.5 && 
                  (avg_connections as f32) < (config.connection_threshold as f32) * 0.5 {
            Some(ScalingAction::ScaleDown(config.scale_down_factor))
        } else {
            None
        }
    }

    pub async fn cleanup_inactive_instances(&self) {
        let mut instances = self.instances.write().await;
        let now = Utc::now();
        
        instances.retain(|_, instance| {
            let age = now - instance.last_heartbeat;
            if age.num_seconds() > 180 { // 3 minutes timeout
                warn!("Removing inactive instance: {}", instance.id);
                false
            } else {
                true
            }
        });
    }

    pub async fn get_instance_count(&self) -> usize {
        self.instances.read().await.len()
    }

    pub async fn get_active_instances(&self) -> Vec<InstanceInfo> {
        self.instances.read().await.values().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub enum ScalingAction {
    ScaleUp(f32),
    ScaleDown(f32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;
    use std::time::Duration;

    #[tokio::test]
    async fn test_instance_registration() {
        let manager = ScalingManager::new(ScalingConfig::default());
        
        let instance_id = manager.register_instance("localhost".to_string(), 8080).await;
        assert_eq!(manager.get_instance_count().await, 1);
        
        let instances = manager.get_active_instances().await;
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].id, instance_id);
    }

    #[tokio::test]
    async fn test_scaling_decision() {
        let manager = ScalingManager::new(ScalingConfig::default());
        let instance_id = manager.register_instance("localhost".to_string(), 8080).await;
        
        // Test scale up condition
        let high_load_metrics = SystemMetrics {
            cpu_usage: 85.0,
            memory_used: 8000,
            memory_total: 10000,
            connection_count: 1200,
            active_users: 1000,
            request_rate: 100.0,
            error_rate: 0.1,
            response_time_p95: 0.5,
            timestamp: Utc::now(),
        };
        
        manager.update_instance_metrics(instance_id, high_load_metrics).await.unwrap();
        
        if let Some(ScalingAction::ScaleUp(_)) = manager.check_scaling_needs().await {
            // Expected
        } else {
            panic!("Expected scale up action");
        }
        
        // Test scale down condition
        let low_load_metrics = SystemMetrics {
            cpu_usage: 20.0,
            memory_used: 2000,
            memory_total: 10000,
            connection_count: 100,
            active_users: 50,
            request_rate: 10.0,
            error_rate: 0.0,
            response_time_p95: 0.1,
            timestamp: Utc::now(),
        };
        
        manager.update_instance_metrics(instance_id, low_load_metrics).await.unwrap();
        
        // Wait for cooldown
        sleep(Duration::from_secs(1)).await;
        
        if let Some(ScalingAction::ScaleDown(_)) = manager.check_scaling_needs().await {
            // Expected
        } else {
            panic!("Expected scale down action");
        }
    }

    #[tokio::test]
    async fn test_cleanup_inactive_instances() {
        let manager = ScalingManager::new(ScalingConfig::default());
        
        // Register an instance
        let instance_id = manager.register_instance("localhost".to_string(), 8080).await;
        assert_eq!(manager.get_instance_count().await, 1);
        
        // Wait for instance to become inactive
        sleep(Duration::from_secs(4)).await;
        
        // Cleanup inactive instances
        manager.cleanup_inactive_instances().await;
        
        // Verify instance was removed
        assert_eq!(manager.get_instance_count().await, 0);
    }
}
