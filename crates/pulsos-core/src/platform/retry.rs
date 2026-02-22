use crate::error::PulsosError;
use std::time::Duration;

/// Send an HTTP request with a single automatic retry on 429 or 5xx responses.
///
/// - **429 Too Many Requests**: reads the `Retry-After` header (defaults to 5 s), sleeps, retries once.
/// - **5xx Server Error**: sleeps 2 s, retries once.
/// - All other responses are returned immediately (success or caller-handled error).
///
/// If the request body cannot be cloned (e.g., streaming body), retry is skipped.
pub async fn send_with_retry(
    req: reqwest::RequestBuilder,
    platform: &str,
) -> Result<reqwest::Response, PulsosError> {
    // Clone before sending so we have a copy for the potential retry.
    let retry_req = req.try_clone();

    let resp = req.send().await.map_err(|e| PulsosError::Network {
        platform: platform.to_string(),
        message: e.to_string(),
        source: Some(e),
    })?;

    let status = resp.status();

    if let Some(retry) = retry_req {
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let delay_secs = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5)
                .min(60);
            tracing::warn!(
                platform = platform,
                delay_secs = delay_secs,
                "Rate limited (429), retrying after delay"
            );
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;
            return retry.send().await.map_err(|e| PulsosError::Network {
                platform: platform.to_string(),
                message: e.to_string(),
                source: Some(e),
            });
        } else if status.is_server_error() {
            tracing::warn!(
                platform = platform,
                status = %status,
                "Server error (5xx), retrying after 2s"
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
            return retry.send().await.map_err(|e| PulsosError::Network {
                platform: platform.to_string(),
                message: e.to_string(),
                source: Some(e),
            });
        }
    }

    Ok(resp)
}
