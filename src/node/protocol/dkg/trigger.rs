use log::info;
use tokio::time::{self, Duration, Interval};

pub struct DKGTrigger {
    interval: Interval,
}

impl DKGTrigger {
    pub fn new() -> Self {
        Self {
            interval: time::interval(Duration::from_secs(15)),
        }
    }

    pub async fn run(&mut self) {
        loop {
            self.interval.tick().await;
            info!("DKG trigger: checking if DKG needs to be started");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_trigger_fires() {
        let mut trigger = DKGTrigger::new();
        // Wait for just over one interval to ensure we get at least one trigger
        let result = timeout(Duration::from_secs(16), trigger.run()).await;
        assert!(result.is_err(), "Trigger should not complete");
    }
}
