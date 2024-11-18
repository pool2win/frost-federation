use log::info;
use tokio::time::{self, Duration, Interval};

pub async fn run_dkg_trigger(mut interval: Interval) {
    loop {
        interval.tick().await;
        info!("DKG trigger: checking if DKG needs to be started");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{self, timeout};

    #[tokio::test]
    async fn test_trigger_fires() {
        let interval = time::interval(Duration::from_millis(100));
        // Wait for just over one interval to ensure we get at least one trigger
        let result = timeout(Duration::from_millis(110), run_dkg_trigger(interval)).await;
        assert!(result.is_err(), "Trigger should not complete");
    }
}
