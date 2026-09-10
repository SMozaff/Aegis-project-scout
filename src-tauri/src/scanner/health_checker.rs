use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::time::{timeout, Duration};
use std::sync::Arc;

use crate::models::endpoint::HealthStatus;

const MAX_CONCURRENT: usize = 10;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;

pub async fn check_health(endpoint: &str) -> Option<HealthStatus> {
    let client = match Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return Some(HealthStatus {
                endpoint: endpoint.to_string(),
                reachable: false,
                success: false,
                status_code: None,
                response_time_ms: None,
                error: Some(error.to_string()),
            })
        }
    };

    let start = Instant::now();

    let result = timeout(
        Duration::from_secs(REQUEST_TIMEOUT_SECONDS),
        client.get(endpoint).send(),
    )
    .await;

    match result {
        Ok(Ok(response)) => {
            let status = response.status();

            Some(HealthStatus {
                endpoint: endpoint.to_string(),
                reachable: true,
                success: status.is_success(),
                status_code: Some(status.as_u16()),
                response_time_ms: Some(start.elapsed().as_millis()),
                error: None,
            })
        }
        Ok(Err(error)) => Some(HealthStatus {
            endpoint: endpoint.to_string(),
            reachable: false,
            success: false,
            status_code: None,
            response_time_ms: Some(start.elapsed().as_millis()),
            error: Some(error.to_string()),
        }),
        Err(_) => Some(HealthStatus {
            endpoint: endpoint.to_string(),
            reachable: false,
            success: false,
            status_code: None,
            response_time_ms: Some(start.elapsed().as_millis()),
            error: Some("request timeout".into()),
        }),
    }
}

pub async fn batch_check(endpoints: &[String]) -> Vec<HealthStatus> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut tasks = Vec::new();

    for endpoint in endpoints.iter().cloned() {
        let semaphore = semaphore.clone();

        tasks.push(tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await.ok();
            check_health(&endpoint).await
        }));
    }

    let mut results = Vec::new();

    for task in tasks {
        if let Ok(Some(status)) = task.await {
            results.push(status);
        }
    }

    results
}
