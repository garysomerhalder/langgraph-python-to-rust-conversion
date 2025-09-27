use std::collections::HashMap;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct StateChannels<T>
where
    T: Clone + Send + Sync + 'static,
{
    channels: HashMap<String, mpsc::Sender<T>>,
    receivers: HashMap<String, mpsc::Receiver<T>>,
}

impl<T> StateChannels<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            receivers: HashMap::new(),
        }
    }

    pub fn create_channel(&mut self, name: String, buffer_size: usize) {
        let (tx, rx) = mpsc::channel(buffer_size);
        self.channels.insert(name.clone(), tx);
        self.receivers.insert(name, rx);
    }

    pub async fn send(
        &self,
        channel_name: &str,
        value: T,
    ) -> Result<(), mpsc::error::SendError<T>> {
        if let Some(sender) = self.channels.get(channel_name) {
            sender.send(value).await
        } else {
            Err(mpsc::error::SendError(value))
        }
    }

    pub async fn receive(&mut self, channel_name: &str) -> Option<T> {
        if let Some(receiver) = self.receivers.get_mut(channel_name) {
            receiver.recv().await
        } else {
            None
        }
    }

    pub fn get_sender(&self, channel_name: &str) -> Option<mpsc::Sender<T>> {
        self.channels.get(channel_name).cloned()
    }
}

impl<T> Clone for StateChannels<T>
where
    T: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            channels: self.channels.clone(),
            receivers: HashMap::new(), // Can't clone receivers
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_channel_creation() {
        let mut channels: StateChannels<String> = StateChannels::new();
        channels.create_channel("test".to_string(), 10);

        assert!(channels.get_sender("test").is_some());
    }

    #[tokio::test]
    async fn test_send_receive() {
        let mut channels: StateChannels<String> = StateChannels::new();
        channels.create_channel("test".to_string(), 10);

        let result = channels.send("test", "Hello".to_string()).await;
        assert!(result.is_ok());

        let received = channels.receive("test").await;
        assert_eq!(received, Some("Hello".to_string()));
    }
}
