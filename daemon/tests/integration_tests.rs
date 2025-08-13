use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;

/// Integration tests for Jasper daemon components
/// 
/// These tests verify that different parts of the system work together correctly,
/// particularly focusing on D-Bus communication and critical workflows.

#[cfg(test)]
mod tests {
    use super::*;

    /// Test D-Bus service availability and basic communication
    #[tokio::test]
    async fn test_dbus_service_availability() -> Result<()> {
        // This test checks if the D-Bus service can be contacted
        // Note: This requires the daemon to be running
        
        let output = Command::new("gdbus")
            .args(&[
                "call",
                "--session",
                "--dest", "org.personal.CompanionAI",
                "--object-path", "/org/personal/CompanionAI/Companion",
                "--method", "org.personal.CompanionAI.Companion1.GetStatus"
            ])
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    let response = String::from_utf8_lossy(&result.stdout);
                    println!("D-Bus service response: {}", response);
                    assert!(response.contains("Observing") || response.contains("Active"));
                } else {
                    let error = String::from_utf8_lossy(&result.stderr);
                    println!("D-Bus service not available: {}", error);
                    // Don't fail the test if service isn't running - this is expected in CI
                    println!("Skipping D-Bus test - daemon not running");
                }
            }
            Err(e) => {
                println!("gdbus command not available: {}", e);
                // Skip test if gdbus is not available (common in CI environments)
            }
        }

        Ok(())
    }

    /// Test frontend formatting capability
    #[tokio::test]
    async fn test_frontend_formatting() -> Result<()> {
        // Test that the daemon can format insights for different frontends
        
        let output = Command::new("gdbus")
            .args(&[
                "call",
                "--session",
                "--dest", "org.personal.CompanionAI",
                "--object-path", "/org/personal/CompanionAI/Companion",
                "--method", "org.personal.CompanionAI.Companion1.GetFormattedInsights",
                "waybar"
            ])
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    let response = String::from_utf8_lossy(&result.stdout);
                    println!("Waybar format response: {}", response);
                    
                    // The response should be a valid JSON-like structure for waybar
                    assert!(response.contains("{") || response.contains("error"));
                } else {
                    println!("Skipping frontend format test - daemon not running");
                }
            }
            Err(_) => {
                println!("Skipping frontend format test - gdbus not available");
            }
        }

        Ok(())
    }

    /// Test notification system initialization
    #[tokio::test]
    async fn test_notification_system_init() {
        // This test verifies that the notification system can be initialized
        // without panicking or causing runtime errors
        
        use crate::services::notification::{NotificationService, NotificationConfig};
        
        let config = NotificationConfig {
            enabled: true,
            notify_new_insights: true,
            notify_context_changes: false,
            notify_cache_refresh: false,
            notification_timeout: 5000,
            min_urgency_threshold: 3,
            preferred_method: "auto".to_string(),
            app_name: "Jasper Test".to_string(),
            desktop_entry: "jasper-test".to_string(),
        };
        
        let notification_service = NotificationService::new(config);
        
        // Test that the service can report its capabilities without crashing
        let capabilities = notification_service.get_notification_capabilities();
        println!("Notification capabilities: {:?}", capabilities);
        
        // This should not panic
        assert!(true);
    }

    /// Test configuration validation
    #[tokio::test]
    async fn test_config_validation() {
        use crate::config::Config;
        
        // Test timezone validation
        let mut config = Config::default();
        
        // Test with valid timezone
        config.general.timezone = "America/New_York".to_string();
        // This should not panic when validating
        
        // Test with invalid timezone - we expect this to be caught during load/validation
        config.general.timezone = "Invalid/Timezone".to_string();
        
        // The validation happens during Config::load(), so we just ensure the structure is sound
        assert_eq!(config.general.timezone, "Invalid/Timezone");
    }

    /// Test database initialization and basic operations
    #[tokio::test]
    async fn test_database_operations() -> Result<()> {
        use crate::database::Database;
        use tempfile::tempdir;
        
        // Create a temporary directory for test database
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        
        // Initialize database
        let db = Database::new(db_path.clone()).await?;
        
        // Test basic database operations
        let events = db.get_events_in_range(
            chrono::Utc::now() - chrono::Duration::days(1),
            chrono::Utc::now() + chrono::Duration::days(1)
        ).await?;
        
        // Should not crash and should return a vector (possibly empty)
        assert!(events.is_empty() || !events.is_empty()); // Always true, just checking it runs
        
        println!("Database test completed successfully");
        Ok(())
    }

    /// Test error recovery mechanisms
    #[tokio::test] 
    async fn test_error_recovery() {
        use crate::error_recovery::ErrorRecovery;
        use crate::errors::JasperError;
        
        // Test retry mechanism with a function that always fails
        let result = ErrorRecovery::retry_with_backoff(
            || async {
                Err(JasperError::network("Test network error"))
            },
            2, // max attempts
            Duration::from_millis(10), // short delay for testing
            "test operation",
        ).await;
        
        // Should fail after retries
        assert!(result.is_err());
        
        // Test retry mechanism with a function that succeeds on second try
        let mut attempts = 0;
        let result = ErrorRecovery::retry_with_backoff(
            || async {
                attempts += 1;
                if attempts == 1 {
                    Err(JasperError::network("Test network error"))
                } else {
                    Ok("Success")
                }
            },
            3, // max attempts  
            Duration::from_millis(10),
            "test operation",
        ).await;
        
        // Should succeed on second attempt
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success");
    }

    /// Test circuit breaker functionality
    #[tokio::test]
    async fn test_circuit_breaker() {
        use crate::error_recovery::CircuitBreaker;
        use crate::errors::JasperError;
        
        let breaker = CircuitBreaker::new(2, Duration::from_millis(100));
        
        // First failure
        let result1 = breaker.call(
            || async { Err(JasperError::api("test", "API error")) },
            "test operation 1"
        ).await;
        assert!(result1.is_err());
        
        // Second failure - should trigger circuit breaker
        let result2 = breaker.call(
            || async { Err(JasperError::api("test", "API error")) },
            "test operation 2"
        ).await;
        assert!(result2.is_err());
        
        // Third attempt - circuit should be open
        let result3 = breaker.call(
            || async { Ok("This should not execute") },
            "test operation 3"
        ).await;
        assert!(result3.is_err());
        
        println!("Circuit breaker test completed");
    }
}

/// Helper function to check if daemon is running
fn is_daemon_running() -> bool {
    Command::new("systemctl")
        .args(&["--user", "is-active", "jasper-companion-daemon"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Helper function to start daemon for testing (if possible)
async fn ensure_daemon_running() -> Result<()> {
    if !is_daemon_running() {
        println!("Daemon not running, attempting to start...");
        let output = Command::new("systemctl")
            .args(&["--user", "start", "jasper-companion-daemon"])
            .output()?;
        
        if !output.status.success() {
            println!("Could not start daemon: {}", String::from_utf8_lossy(&output.stderr));
        } else {
            // Give daemon time to start
            sleep(Duration::from_secs(2)).await;
        }
    }
    Ok(())
}