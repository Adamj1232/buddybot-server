use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use crate::error::Error;
use tracing::{error, info};

#[derive(Debug)]
pub struct ConnectionPool {
    connections: Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add(&self, id: Uuid, sender: mpsc::UnboundedSender<Message>) {
        self.connections.write().await.insert(id, sender);
        info!("Added connection {} to pool", id);
    }

    pub async fn remove(&self, id: &Uuid) -> bool {
        let removed = self.connections.write().await.remove(id).is_some();
        if removed {
            info!("Removed connection {} from pool", id);
        }
        removed
    }

    pub async fn broadcast(&self, msg: &str, exclude_id: Option<Uuid>) -> Result<(), Error> {
        let connections = self.connections.read().await;
        let message = Message::Text(msg.to_string());

        for (id, sender) in connections.iter() {
            if let Some(exclude) = exclude_id {
                if *id == exclude {
                    continue;
                }
            }

            if let Err(e) = sender.send(message.clone()) {
                error!("Failed to broadcast to connection {}: {}", id, e);
            }
        }

        Ok(())
    }

    pub async fn send_to(&self, id: &Uuid, msg: &str) -> Result<(), Error> {
        if let Some(sender) = self.connections.read().await.get(id) {
            sender
                .send(Message::Text(msg.to_string()))
                .map_err(|e| Error::External(format!("Failed to send message: {}", e)))?;
            Ok(())
        } else {
            Err(Error::External("Connection not found".to_string()))
        }
    }

    pub async fn send_to_many(&self, ids: &[Uuid], msg: &str) -> Result<(), Error> {
        let connections = self.connections.read().await;
        let message = Message::Text(msg.to_string());

        for id in ids {
            if let Some(sender) = connections.get(id) {
                if let Err(e) = sender.send(message.clone()) {
                    error!("Failed to send to connection {}: {}", id, e);
                }
            }
        }

        Ok(())
    }

    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    pub async fn cleanup_inactive(&self, inactive_connections: &[Uuid]) {
        let mut connections = self.connections.write().await;
        for id in inactive_connections {
            connections.remove(id);
            info!("Removed inactive connection {}", id);
        }
    }

    pub async fn get_all_connection_ids(&self) -> Vec<Uuid> {
        self.connections.read().await.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_connection_pool() {
        let pool = ConnectionPool::new();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        // Test adding connections
        pool.add(id1, tx1).await;
        pool.add(id2, tx2).await;
        assert_eq!(pool.connection_count().await, 2);

        // Test broadcasting
        pool.broadcast("test message", None).await.unwrap();
        
        if let Some(Message::Text(msg)) = rx1.try_recv().ok() {
            assert_eq!(msg, "test message");
        } else {
            panic!("Failed to receive broadcast message on connection 1");
        }

        if let Some(Message::Text(msg)) = rx2.try_recv().ok() {
            assert_eq!(msg, "test message");
        } else {
            panic!("Failed to receive broadcast message on connection 2");
        }

        // Test removing connection
        assert!(pool.remove(&id1).await);
        assert_eq!(pool.connection_count().await, 1);

        // Test sending to specific connection
        pool.send_to(&id2, "direct message").await.unwrap();
        if let Some(Message::Text(msg)) = rx2.try_recv().ok() {
            assert_eq!(msg, "direct message");
        } else {
            panic!("Failed to receive direct message");
        }
    }
} 