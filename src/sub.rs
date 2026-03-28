use crate::key::Operation;
use log::{debug, error, info};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::RwLock;

#[derive(Serialize, Clone, Debug)]
pub struct CollectionEvent {
    pub operation: Operation,
    pub key: String,
    pub value: Value,
}

pub struct SubscriptionManager {
    publishers: RwLock<HashMap<String, Sender<CollectionEvent>>>,
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionManager {
    pub fn new() -> Self {
        Self {
            publishers: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_or_create_channel(&self, collection: &str) -> Sender<CollectionEvent> {
        let mut publishers = self.publishers.write().await;

        if let Some(sender) = publishers.get(collection) {
            if sender.receiver_count() > 0 {
                debug!(
                    "Using existing channel for collection '{}' with {} subscribers",
                    collection,
                    sender.receiver_count()
                );
                return sender.clone();
            } else {
                debug!(
                    "Channel for collection '{}' has no subscribers, creating new one",
                    collection
                );
            }
        }

        let (sender, _) = broadcast::channel(20000);
        info!("Created new channel for collection '{}'", collection);
        publishers.insert(collection.to_string(), sender.clone());
        sender
    }

    pub async fn has_subscribers(&self, collection: &str) -> bool {
        let publishers = self.publishers.read().await;
        if let Some(sender) = publishers.get(collection) {
            return sender.receiver_count() > 0;
        }
        false
    }

    pub async fn publish(&self, collection: &str, event: CollectionEvent) {
        if !self.has_subscribers(collection).await {
            return;
        }

        let sender = {
            let publishers = self.publishers.read().await;
            publishers.get(collection).cloned()
        };

        if let Some(sender) = sender {
            debug!(
                "Publishing event for key '{}' to {} subscribers in collection '{}'",
                event.key,
                sender.receiver_count(),
                collection
            );

            match sender.send(event) {
                Ok(n) => debug!("Event sent to {} receivers", n),
                Err(e) => error!("Failed to send event: {:?}", e),
            }
        }
    }
}
