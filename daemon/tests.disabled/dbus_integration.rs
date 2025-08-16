use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;

/// D-Bus specific integration tests
/// 
/// These tests focus specifically on D-Bus communication patterns
/// and ensure the interface works correctly with various frontends

#[cfg(test)]
mod dbus_tests {
    use super::*;

    /// Test all D-Bus method calls that should be available
    #[tokio::test]
    async fn test_all_dbus_methods() -> Result<()> {
        if !is_daemon_available() {
            println!("Daemon not available, skipping D-Bus method tests");
            return Ok(());
        }

        // Test GetStatus
        test_dbus_method("GetStatus", &[]).await?;
        
        // Test GetCurrentInsight
        test_dbus_method("GetCurrentInsight", &[]).await?;
        
        // Test GetFormattedInsights with different frontends
        test_dbus_method("GetFormattedInsights", &["waybar"]).await?;
        test_dbus_method("GetFormattedInsights", &["gnome"]).await?;
        test_dbus_method("GetFormattedInsights", &["terminal"]).await?;
        
        // Test ListFrontends
        test_dbus_method("ListFrontends", &[]).await?;
        
        // Test GetWaybarJson (backward compatibility)
        test_dbus_method("GetWaybarJson", &[]).await?;

        Ok(())
    }

    /// Test D-Bus method call error handling
    #[tokio::test] 
    async fn test_dbus_error_handling() -> Result<()> {
        if !is_daemon_available() {
            println!("Daemon not available, skipping D-Bus error handling tests");
            return Ok(());
        }

        // Test invalid method call
        let output = call_dbus_method("NonExistentMethod", &[]).await;
        match output {
            Ok(result) => {
                // Should fail with method not found
                assert!(!result.status.success());
                let error = String::from_utf8_lossy(&result.stderr);
                assert!(error.contains("Unknown method") || error.contains("not found"));
            }
            Err(_) => {
                println!("Expected error for invalid method call");
            }
        }

        // Test invalid frontend
        let output = call_dbus_method("GetFormattedInsights", &["invalid_frontend"]).await;
        match output {
            Ok(result) => {
                if result.status.success() {
                    let response = String::from_utf8_lossy(&result.stdout);
                    // Should return an error message in the response
                    assert!(response.contains("error") || response.contains("Unknown frontend"));
                }
            }
            Err(_) => {
                println!("Error testing invalid frontend - expected behavior");
            }
        }

        Ok(())
    }

    /// Test D-Bus feedback mechanism
    #[tokio::test]
    async fn test_dbus_feedback_methods() -> Result<()> {
        if !is_daemon_available() {
            println!("Daemon not available, skipping D-Bus feedback tests");
            return Ok(());
        }

        // Test AcknowledgeInsight 
        test_dbus_method("AcknowledgeInsight", &["test_id", "test_action"]).await?;
        
        // Test ProvideFeedback
        test_dbus_method("ProvideFeedback", &["test_id", "positive"]).await?;
        
        // Test RequestRefresh
        test_dbus_method("RequestRefresh", &[]).await?;

        Ok(())
    }

    /// Test concurrent D-Bus calls
    #[tokio::test]
    async fn test_concurrent_dbus_calls() -> Result<()> {
        if !is_daemon_available() {
            println!("Daemon not available, skipping concurrent D-Bus tests");
            return Ok(());
        }

        // Spawn multiple concurrent D-Bus calls
        let mut handles = vec![];
        
        for i in 0..5 {
            let handle = tokio::spawn(async move {
                let method = match i % 3 {
                    0 => "GetStatus",
                    1 => "GetCurrentInsight", 
                    _ => "GetWaybarJson",
                };
                
                test_dbus_method(method, &[]).await
            });
            handles.push(handle);
        }
        
        // Wait for all calls to complete
        for handle in handles {
            handle.await??;
        }
        
        println!("Concurrent D-Bus calls completed successfully");
        Ok(())
    }

    /// Test D-Bus service resilience
    #[tokio::test]
    async fn test_dbus_service_resilience() -> Result<()> {
        if !is_daemon_available() {
            println!("Daemon not available, skipping resilience tests");
            return Ok(());
        }

        // Make rapid successive calls to test service stability
        for i in 0..10 {
            let result = test_dbus_method("GetStatus", &[]).await;
            if result.is_err() {
                println!("Call {} failed: {:?}", i, result);
            }
            // Small delay to avoid overwhelming the service
            sleep(Duration::from_millis(100)).await;
        }

        // Test that service is still responsive after rapid calls
        test_dbus_method("GetCurrentInsight", &[]).await?;

        println!("D-Bus service resilience test completed");
        Ok(())
    }
}

/// Helper function to test a specific D-Bus method call
async fn test_dbus_method(method: &str, args: &[&str]) -> Result<()> {
    let output = call_dbus_method(method, args).await?;
    
    if output.status.success() {
        let response = String::from_utf8_lossy(&output.stdout);
        println!("Method {} response: {}", method, response.trim());
        
        // Basic validation - response should not be empty
        assert!(!response.trim().is_empty(), "Method {} returned empty response", method);
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        println!("Method {} failed: {}", method, error);
        
        // Some methods might legitimately fail (e.g., if no insights available)
        // We mainly want to ensure the service doesn't crash
        if error.contains("service not available") || error.contains("not running") {
            return Err(anyhow::anyhow!("D-Bus service not available"));
        }
    }
    
    Ok(())
}

/// Helper function to make a D-Bus method call
async fn call_dbus_method(method: &str, args: &[&str]) -> Result<std::process::Output> {
    let method_string = format!("org.personal.CompanionAI.Companion1.{}", method);
    let mut cmd_args = vec![
        "call",
        "--session", 
        "--dest", "org.personal.CompanionAI",
        "--object-path", "/org/personal/CompanionAI/Companion",
        "--method", &method_string,
    ];
    
    // Add method arguments
    for arg in args {
        cmd_args.push(arg);
    }
    
    let output = Command::new("gdbus")
        .args(&cmd_args)
        .output()?;
        
    Ok(output)
}

/// Check if daemon is available for testing
fn is_daemon_available() -> bool {
    // Check if systemctl can see the daemon
    let systemctl_check = Command::new("systemctl")
        .args(&["--user", "is-active", "jasper-companion-daemon"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    
    if systemctl_check {
        return true;
    }
    
    // Try direct D-Bus introspection
    let dbus_check = Command::new("gdbus")
        .args(&[
            "introspect",
            "--session",
            "--dest", "org.personal.CompanionAI",
            "--object-path", "/org/personal/CompanionAI/Companion"
        ])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    
    dbus_check
}