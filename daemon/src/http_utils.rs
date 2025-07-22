//! HTTP utility functions for consistent error handling across API clients

use anyhow::{anyhow, Result};
use reqwest::Response;
use tracing::warn;

/// Handle Google API response errors with consistent logging and error formatting
pub async fn handle_google_api_response(response: Response) -> Result<Response> {
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await?;
        warn!("Google Calendar API error: {} - {}", status, error_text);
        return Err(anyhow!("Google Calendar API error: {} - {}", status, error_text));
    }
    Ok(response)
}

/// Handle generic HTTP API response errors
pub async fn handle_api_response(response: Response, api_name: &str) -> Result<Response> {
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await?;
        warn!("{} API error: {} - {}", api_name, status, error_text);
        return Err(anyhow!("{} API error: {} - {}", api_name, status, error_text));
    }
    Ok(response)
}

/// Handle OAuth2 response errors and return response text for debugging
pub async fn handle_oauth2_response_with_text(response: Response) -> Result<String> {
    let status = response.status();
    let response_text = response.text().await?;
    
    if !status.is_success() {
        return Err(anyhow!("Google OAuth2 error: {} - {}", status, response_text));
    }
    
    Ok(response_text)
}

/// Parse JSON response with consistent error handling
pub async fn parse_json_response<T>(response: Response, context: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    response.json().await
        .map_err(|e| anyhow!("Failed to parse {}: {}", context, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Note: These would be integration tests in a real scenario
    // For now, we include them for completeness but they won't run
    // without setting up mock HTTP responses
}