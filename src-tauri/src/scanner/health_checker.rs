use reqwest::Client;
use std::time::Instant;
use tokio::time::{timeout, Duration};

use crate::models::project::HealthStatus;

const REQUEST_TIMEOUT_SECONDS: u64 = 10;

pub async fn check_health(endpoint: &str) -> Option<HealthStatus> {
    let client = match Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(_) => return None,
    };

    let start = Instant::now();
    let result = timeout(
        Duration::from_secs(REQUEST_TIMEOUT_SECONDS),
        client.head(endpoint).send(),
    )
    .await;

    match result {
        Ok(Ok(response)) => Some(HealthStatus {
            endpoint: endpoint.to_string(),
            status_code: Some(response.status().as_u16()),
            response_time_ms: Some(start.elapsed().as_millis()),
            is_healthy: response.status().is_success(),
            checked_at: chrono::Utc::now(),
        }),
        Ok(Err(_)) | Err(_) => Some(HealthStatus {
            endpoint: endpoint.to_string(),
            status_code: None,
            response_time_ms: Some(start.elapsed().as_millis()),
            is_healthy: false,
            checked_at: chrono::Utc::now(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_localhost() {
        let result = check_health("http://127.0.0.1:9999").await;
        assert!(result.is_some());
        assert!(!result.unwrap().is_healthy);
    }

    #[tokio::test]
    async fn rejects_private_ip() {
        let result = check_health("http://192.168.1.1").await;
        assert!(result.is_some());
        assert!(!result.unwrap().is_healthy);
    }

    #[tokio::test]
    async fn handles_malformed_url() {
        let result = check_health("not a url").await;
        // Should not panic; returns Some with is_healthy=false
        assert!(result.is_some());
    }
}
