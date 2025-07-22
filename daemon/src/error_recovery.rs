use std::time::Duration;
use tokio::time::sleep;
use tracing::{warn, debug, error};

use crate::errors::{JasperError, JasperResult, ErrorCategory};

/// Error recovery strategies for common failure scenarios
pub struct ErrorRecovery;

impl ErrorRecovery {
    /// Retry an operation with exponential backoff
    pub async fn retry_with_backoff<F, Fut, T>(
        operation: F,
        max_attempts: usize,
        base_delay: Duration,
        operation_name: &str,
    ) -> JasperResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = JasperResult<T>>,
    {
        let mut attempts = 0;
        let mut delay = base_delay;
        
        loop {
            attempts += 1;
            
            match operation().await {
                Ok(result) => {
                    if attempts > 1 {
                        debug!("{} succeeded after {} attempts", operation_name, attempts);
                    }
                    return Ok(result);
                }
                Err(error) => {
                    if attempts >= max_attempts {
                        error!("{} failed after {} attempts: {}", operation_name, attempts, error);
                        return Err(error);
                    }
                    
                    if !error.is_recoverable() {
                        error!("{} failed with non-recoverable error: {}", operation_name, error);
                        return Err(error);
                    }
                    
                    warn!(
                        "{} failed (attempt {}/{}): {}. Retrying in {:?}",
                        operation_name, attempts, max_attempts, error, delay
                    );
                    
                    sleep(delay).await;
                    delay = std::cmp::min(delay * 2, Duration::from_secs(60)); // Cap at 60 seconds
                }
            }
        }
    }
    
    /// Handle API rate limiting with appropriate delays
    pub async fn handle_rate_limit(
        service: &str,
        retry_after: Option<Duration>,
    ) -> JasperResult<()> {
        let delay = retry_after.unwrap_or(Duration::from_secs(60));
        
        warn!(
            "Rate limited by {} API, waiting {:?} before retry",
            service, delay
        );
        
        sleep(delay).await;
        Ok(())
    }
    
    /// Gracefully handle network connectivity issues
    pub async fn handle_network_error(
        error: &JasperError,
        operation_name: &str,
    ) -> JasperResult<()> {
        match error.category() {
            ErrorCategory::Network => {
                warn!("Network error during {}: {}. Will retry...", operation_name, error);
                sleep(Duration::from_secs(5)).await;
                Ok(())
            }
            ErrorCategory::Timeout => {
                warn!("Timeout during {}: {}. Will retry with longer timeout...", operation_name, error);
                sleep(Duration::from_secs(10)).await;
                Ok(())
            }
            _ => Err(error.clone()),
        }
    }
    
    /// Handle authentication errors with user-friendly messages
    pub fn handle_auth_error(error: &JasperError, service: &str) -> JasperResult<String> {
        match error.category() {
            ErrorCategory::Authentication => {
                let message = match service {
                    "google" => {
                        "Google Calendar authentication failed. Please run 'jasper-companion-daemon auth-google' to re-authenticate."
                    }
                    "claude" => {
                        "Claude API authentication failed. Please check your API key with 'jasper-companion-daemon set-api-key <key>'."
                    }
                    _ => "Authentication failed. Please check your credentials."
                };
                Ok(message.to_string())
            }
            _ => Err(error.clone()),
        }
    }
    
    /// Handle database errors with recovery suggestions
    pub fn handle_database_error(error: &JasperError) -> JasperResult<String> {
        match error.category() {
            ErrorCategory::Database => {
                let suggestion = match error {
                    JasperError::Database { operation, .. } if operation.contains("migration") => {
                        "Database migration failed. Try deleting the database file to recreate it."
                    }
                    JasperError::Database { operation, .. } if operation.contains("connection") => {
                        "Database connection failed. Check file permissions and disk space."
                    }
                    JasperError::Database { operation, .. } if operation.contains("transaction") => {
                        "Database transaction failed. The operation will be retried."
                    }
                    _ => "Database operation failed. This may be a temporary issue."
                };
                Ok(suggestion.to_string())
            }
            _ => Err(error.clone()),
        }
    }
    
}

/// Circuit breaker pattern for failing services
pub struct CircuitBreaker {
    failure_count: std::sync::atomic::AtomicUsize,
    last_failure: std::sync::Mutex<Option<std::time::Instant>>,
    failure_threshold: usize,
    timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, timeout: Duration) -> Self {
        Self {
            failure_count: std::sync::atomic::AtomicUsize::new(0),
            last_failure: std::sync::Mutex::new(None),
            failure_threshold,
            timeout,
        }
    }
    
    pub fn is_open(&self) -> bool {
        let failure_count = self.failure_count.load(std::sync::atomic::Ordering::Relaxed);
        
        if failure_count < self.failure_threshold {
            return false;
        }
        
        let last_failure = self.last_failure.lock().unwrap();
        if let Some(last_failure_time) = *last_failure {
            last_failure_time.elapsed() < self.timeout
        } else {
            false
        }
    }
    
    pub fn record_success(&self) {
        self.failure_count.store(0, std::sync::atomic::Ordering::Relaxed);
        let mut last_failure = self.last_failure.lock().unwrap();
        *last_failure = None;
    }
    
    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut last_failure = self.last_failure.lock().unwrap();
        *last_failure = Some(std::time::Instant::now());
    }
    
    pub async fn call<F, Fut, T>(&self, operation: F, operation_name: &str) -> JasperResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = JasperResult<T>>,
    {
        if self.is_open() {
            warn!("Circuit breaker is open for {}, skipping operation", operation_name);
            return Err(JasperError::service_unavailable(operation_name));
        }
        
        match operation().await {
            Ok(result) => {
                self.record_success();
                Ok(result)
            }
            Err(error) => {
                self.record_failure();
                Err(error)
            }
        }
    }
}

/// Macro for easy retry with exponential backoff
#[macro_export]
macro_rules! retry_operation {
    ($operation:expr, $name:expr) => {
        $crate::error_recovery::ErrorRecovery::retry_with_backoff(
            || async { $operation },
            3,
            std::time::Duration::from_millis(100),
            $name,
        )
        .await
    };
    
    ($operation:expr, $name:expr, $max_attempts:expr) => {
        $crate::error_recovery::ErrorRecovery::retry_with_backoff(
            || async { $operation },
            $max_attempts,
            std::time::Duration::from_millis(100),
            $name,
        )
        .await
    };
}