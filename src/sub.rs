use crate::key::Operation;
use log::{debug, error, info};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::RwLock;

// Event that will be sent to subscribers
#[derive(Serialize, Clone, Debug)]
pub struct CollectionEvent {
    pub operation: Operation,
    pub key: String,
    pub value: Value,
}

// Subscription manager to handle collection events
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

    // Get or create a channel for a collection
    pub async fn get_or_create_channel(&self, collection: &str) -> Sender<CollectionEvent> {
        let mut publishers = self.publishers.write().await;

        // Check if we already have a channel and it's still active
        if let Some(sender) = publishers.get(collection) {
            // Check if sender is still usable (has active receivers)
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

        // Create a new channel with larger capacity
        let (sender, _) = broadcast::channel(20000); // Increased buffer size
        info!("Created new channel for collection '{}'", collection);
        publishers.insert(collection.to_string(), sender.clone());
        sender
    }

    // Check if a collection has any subscribers without creating a channel
    pub async fn has_subscribers(&self, collection: &str) -> bool {
        let publishers = self.publishers.read().await;
        if let Some(sender) = publishers.get(collection) {
            return sender.receiver_count() > 0;
        }
        false
    }

    // Publish an event to all subscribers of a collection
    pub async fn publish(&self, collection: &str, event: CollectionEvent) {
        if !self.has_subscribers(collection).await {
            return;
        }

        // We know there are subscribers, so get the sender
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

            // Send the event and log any errors
            match sender.send(event) {
                Ok(n) => debug!("Event sent to {} receivers", n),
                Err(e) => error!("Failed to send event: {:?}", e),
            }
        }
    }
}
