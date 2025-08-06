use anyhow::Result;
use chrono::{Utc, Duration};
use crate::database::Database;

pub async fn insert_test_events(database: &Database) -> Result<()> {
    
    // Create a test account and calendars using safe methods
    database.insert_test_account(Utc::now().timestamp())?;
    
    database.insert_test_calendar(1, "personal", "Personal Calendar", "personal", "#4285F4")?;
    database.insert_test_calendar(2, "house", "House Maintenance", "house", "#F4B400")?;
    database.insert_test_calendar(3, "family", "Family Events", "family", "#0F9D58")?;
    
    // Clear existing test events
    database.delete_test_events()?;
    
    let now = Utc::now();
    let tomorrow = now + Duration::days(1);
    let day_after = now + Duration::days(2);
    let three_days = now + Duration::days(3);
    let four_days = now + Duration::days(4);
    let five_days = now + Duration::days(5);
    
    // Insert test events that will create correlations
    
    // 1. Travel + Cleaning conflict
    database.insert_test_event(
        "trip1", 3, "Beach Vacation", "Family trip to the coast",
        three_days.timestamp(), (three_days + Duration::days(3)).timestamp(), "travel"
    )?;
    
    database.insert_test_event(
        "clean1", 2, "Cleaning Crew", "Weekly house cleaning service",
        four_days.timestamp(), (four_days + Duration::hours(3)).timestamp(), "maintenance"
    )?;
    
    // 2. Important meeting with busy day before
    database.insert_test_event(
        "presentation1", 1, "Project Presentation", "Important quarterly review presentation",
        five_days.timestamp(), (five_days + Duration::hours(2)).timestamp(), "meeting"
    )?;
    
    database.insert_test_event(
        "meeting1", 1, "Late Strategy Meeting", "End of day strategy session",
        (five_days - Duration::hours(8)).timestamp(), (five_days - Duration::hours(6)).timestamp(), "meeting"
    )?;
    
    // 3. Overcommitted day
    let busy_day = tomorrow;
    database.insert_test_event(
        "busy1", 1, "Morning Standup", "Daily team meeting",
        busy_day.timestamp(), (busy_day + Duration::hours(1)).timestamp(), "meeting"
    )?;
    
    database.insert_test_event(
        "busy2", 1, "Client Call", "Important client check-in",
        (busy_day + Duration::hours(2)).timestamp(), (busy_day + Duration::hours(3)).timestamp(), "meeting"
    )?;
    
    database.insert_test_event(
        "busy3", 1, "Code Review", "Team code review session",
        (busy_day + Duration::hours(4)).timestamp(), (busy_day + Duration::hours(5)).timestamp(), "meeting"
    )?;
    
    database.insert_test_event(
        "busy4", 1, "Design Workshop", "UX design collaboration",
        (busy_day + Duration::hours(6)).timestamp(), (busy_day + Duration::hours(8)).timestamp(), "meeting"
    )?;
    
    database.insert_test_event(
        "busy5", 1, "Evening Retrospective", "Sprint retrospective meeting",
        (busy_day + Duration::hours(9)).timestamp(), (busy_day + Duration::hours(10)).timestamp(), "meeting"
    )?;
    
    // 4. Another travel scenario
    database.insert_test_event(
        "delivery1", 2, "Package Delivery", "Important package expected",
        day_after.timestamp(), (day_after + Duration::hours(1)).timestamp(), "maintenance"
    )?;
    
    println!("✅ Inserted test events:");
    println!("   - Beach vacation (family calendar) with cleaning crew conflict");
    println!("   - Important presentation with busy preparation day");  
    println!("   - Overcommitted day with 5 meetings");
    println!("   - Package delivery scenario");
    
    Ok(())
}