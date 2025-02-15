use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::RwLock;

// Event that will be sent to subscribers
#[derive(Clone, Serialize)]
pub struct CollectionEvent {
    pub operation: String,
    pub key: String,
    pub value: Value,
}

// Subscription manager to handle collection events
pub struct SubscriptionManager {
    publishers: RwLock<HashMap<String, Sender<CollectionEvent>>>,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        Self {
            publishers: RwLock::new(HashMap::new()),
        }
    }

    // Get or create a channel for a collection
    pub async fn get_or_create_channel(&self, collection: &str) -> Sender<CollectionEvent> {
        let mut publishers = self.publishers.write().await;
        if let Some(sender) = publishers.get(collection) {
            sender.clone()
        } else {
            let (sender, _) = broadcast::channel(100); // Buffer size of 100 events
            publishers.insert(collection.to_string(), sender.clone());
            sender
        }
    }

    // Publish an event to all subscribers of a collection
    pub async fn publish(&self, collection: &str, event: CollectionEvent) {
        let sender = self.get_or_create_channel(collection).await;
        let _ = sender.send(event); // Ignore send errors (no subscribers)
    }
}
