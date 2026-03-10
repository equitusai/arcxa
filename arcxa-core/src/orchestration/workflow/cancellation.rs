use tokio::sync::watch;

/// Token for cancelling workflow execution
/// Uses a watch channel for efficient broadcast of cancellation signal
#[derive(Clone, Debug)]
pub struct CancellationToken {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl CancellationToken {
    /// Create a new cancellation token
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    /// Request cancellation of the workflow
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }

    /// Check if cancellation has been requested (non-blocking)
    pub fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    /// Wait for cancellation signal (async)
    pub async fn cancelled(&mut self) {
        let _ = self.receiver.changed().await;
    }

    /// Get a receiver for checking cancellation
    pub fn receiver(&self) -> watch::Receiver<bool> {
        self.receiver.clone()
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_cancellation_token_basic() {
        let token = CancellationToken::new();

        // Initially not cancelled
        assert!(!token.is_cancelled());

        // Cancel the token
        token.cancel();

        // Should be cancelled now
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancellation_token_clone() {
        let token = CancellationToken::new();
        let mut token_clone = token.clone();

        // Cancel original
        token.cancel();

        // Clone should also see cancellation
        assert!(token_clone.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancellation_wait() {
        let token = CancellationToken::new();
        let mut token_clone = token.clone();

        // Spawn task that waits for cancellation
        let handle = tokio::spawn(async move {
            token_clone.cancelled().await;
            "cancelled"
        });

        // Give it a moment to start waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Cancel the token
        token.cancel();

        // Task should complete quickly
        let result = timeout(Duration::from_secs(1), handle)
            .await
            .expect("Task should complete")
            .expect("Task should not panic");

        assert_eq!(result, "cancelled");
    }

    #[tokio::test]
    async fn test_multiple_receivers() {
        let token = CancellationToken::new();
        let mut receiver1 = token.receiver();
        let mut receiver2 = token.receiver();

        // Cancel the token
        token.cancel();

        // Both receivers should see the change
        assert!(receiver1.changed().await.is_ok());
        assert!(receiver2.changed().await.is_ok());
        assert!(*receiver1.borrow());
        assert!(*receiver2.borrow());
    }
}
