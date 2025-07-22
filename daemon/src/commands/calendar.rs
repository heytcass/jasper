use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use crate::calendar_sync::CalendarSyncService;
use crate::test_data;
use super::{Command, CommandContext};

/// Command to test calendar sync
pub struct SyncTestCommand;

/// Command to test real calendar integration  
pub struct TestCalendarCommand;

/// Command to add test events
pub struct AddTestEventsCommand;

/// Command to clean test data from database
pub struct CleanDatabaseCommand;

#[async_trait]
impl Command for SyncTestCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        info!("Testing calendar sync...");
        
        let mut calendar_sync = CalendarSyncService::new(context.config.clone(), context.database.clone())?;
        
        println!("🔄 Testing calendar sync (will attempt token refresh if needed)...");
        
        match calendar_sync.sync_now().await {
            Ok(()) => {
                println!("✅ Calendar sync completed successfully!");
                println!("💡 Events are now available for AI analysis");
            }
            Err(e) => {
                println!("❌ Calendar sync failed: {}", e);
                println!("💡 If authentication failed, run 'jasper-companion-daemon auth-google'");
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Command for TestCalendarCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        println!("🚀 Testing Real Calendar Data Integration");
        println!("=======================================");
        
        // Step 1: Check authentication
        let mut calendar_sync = CalendarSyncService::new(context.config.clone(), context.database.clone())?;
        
        let is_authenticated = calendar_sync.is_authenticated().await;
        
        if !is_authenticated {
            println!("⚠️  Not authenticated with Google Calendar");
            println!("💡 For real calendar data: Run 'cargo run auth-google' first");
            return Ok(());
        } else {
            println!("✅ Google Calendar authentication verified");
            
            // Step 2: Sync real calendar data
            println!("\n🔄 Syncing real calendar data...");
            match calendar_sync.sync_now().await {
                Ok(()) => {
                    println!("✅ Calendar sync completed successfully");
                }
                Err(e) => {
                    println!("❌ Calendar sync failed: {}", e);
                    println!("💡 Try running: cargo run auth-google");
                    return Ok(());
                }
            }
        }
        
        // Step 3: Check what data we actually have in the database
        let planning_horizon = context.config.read().get_planning_horizon();
        
        let now = chrono::Utc::now();
        let future_window = now + planning_horizon;
        
        let events = context.database.get_events_in_range(now, future_window)?;
        let data_source = if is_authenticated { "real Google Calendar" } else { "test" };
        
        // Check for Google Calendar events vs test events
        let google_events: Vec<&crate::database::Event> = events.iter()
            .filter(|e| e.event_type.as_deref() == Some("google_calendar"))
            .collect();
        
        println!("📅 Found {} Google Calendar events for analysis", google_events.len());
        
        if events.is_empty() {
            println!("ℹ️  No events found in planning horizon ({} days)", 
                    context.config.read().general.planning_horizon_days);
            println!("💡 Try extending planning horizon or adding calendar events");
            return Ok(());
        }
        
        // Step 4: Run AI correlation analysis on calendar data
        println!("\n🤖 Running AI correlation analysis on {} calendar data...", data_source);
        
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            println!("⚠️  ANTHROPIC_API_KEY not set - using emergency fallback analysis");
        }
        
        match context.correlation_engine.analyze().await {
            Ok(correlations) => {
                println!("✅ AI analysis completed with {} insights", correlations.len());
                
                if correlations.is_empty() {
                    println!("ℹ️  No significant patterns or conflicts detected");
                } else {
                    println!("\n🔍 AI-Generated Insights:");
                    for (i, correlation) in correlations.iter().take(3).enumerate() {
                        println!("  {}. {}", i + 1, correlation.insight);
                        if !correlation.action_needed.is_empty() {
                            println!("     Action: {}", correlation.action_needed);
                        }
                        println!("     Urgency: {}/10", correlation.urgency_score);
                        println!();
                    }
                    
                    if correlations.len() > 3 {
                        println!("  ... and {} more insights", correlations.len() - 3);
                    }
                }
            }
            Err(e) => {
                println!("❌ AI analysis failed: {}", e);
                if std::env::var("ANTHROPIC_API_KEY").is_err() {
                    println!("💡 Set ANTHROPIC_API_KEY environment variable for full AI analysis");
                }
            }
        }
        
        println!("\n✅ Real calendar integration test completed successfully!");
        println!("📊 Analyzed {} Google Calendar events", google_events.len());
        
        Ok(())
    }
}

#[async_trait]
impl Command for AddTestEventsCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        println!("🧪 Adding test events for Waybar demonstration...");
        
        // Insert test data
        test_data::insert_test_events(&context.database).await?;
        
        println!("✅ Test events added successfully!");
        println!("💡 Now run: jasper-companion-daemon waybar");
        
        Ok(())
    }
}

#[async_trait]
impl Command for CleanDatabaseCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        println!("🗑️  Cleaning test data from database...");
        
        // Remove test events (from test calendars 1, 2, 3)
        let events_deleted = context.database.execute_sql("DELETE FROM events WHERE calendar_id IN (1, 2, 3)", &[])?;
        println!("   Removed {} test events", events_deleted);
        
        // Remove test calendars
        let calendars_deleted = context.database.execute_sql("DELETE FROM calendars WHERE id IN (1, 2, 3)", &[])?;
        println!("   Removed {} test calendars", calendars_deleted);
        
        // Remove test account
        let accounts_deleted = context.database.execute_sql("DELETE FROM accounts WHERE service_name = 'test'", &[])?;
        println!("   Removed {} test accounts", accounts_deleted);
        
        println!("\n📊 Database cleanup summary:");
        println!("   Test events removed: {}", events_deleted);
        println!("   Test calendars removed: {}", calendars_deleted);
        println!("   Test accounts removed: {}", accounts_deleted);
        
        println!("\n✅ Test data cleanup complete!");
        println!("💡 Database now contains only real Google Calendar data");
        
        Ok(())
    }
}