use anyhow::Result;
use chrono::{Utc, Duration};
use crate::database::Database;

pub async fn insert_test_events(database: &Database) -> Result<()> {
    
    // Create a test account and calendar
    database.execute_sql(
        "INSERT OR REPLACE INTO accounts (id, service_name, user_identifier, encrypted_refresh_token, last_sync_timestamp)
         VALUES (1, 'test', 'test_user', 'dummy_token', ?)",
        &[&Utc::now().timestamp()]
    )?;
    
    database.execute_sql(
        "INSERT OR REPLACE INTO calendars (id, account_id, calendar_id, calendar_name, calendar_type, color)
         VALUES (1, 1, 'personal', 'Personal Calendar', 'personal', '#4285F4')",
        &[]
    )?;
    
    database.execute_sql(
        "INSERT OR REPLACE INTO calendars (id, account_id, calendar_id, calendar_name, calendar_type, color)
         VALUES (2, 1, 'house', 'House Maintenance', 'house', '#F4B400')",
        &[]
    )?;
    
    database.execute_sql(
        "INSERT OR REPLACE INTO calendars (id, account_id, calendar_id, calendar_name, calendar_type, color)
         VALUES (3, 1, 'family', 'Family Events', 'family', '#0F9D58')",
        &[]
    )?;
    
    // Clear existing test events
    database.execute_sql("DELETE FROM events WHERE calendar_id IN (1, 2, 3)", &[])?;
    
    let now = Utc::now();
    let tomorrow = now + Duration::days(1);
    let day_after = now + Duration::days(2);
    let three_days = now + Duration::days(3);
    let four_days = now + Duration::days(4);
    let five_days = now + Duration::days(5);
    
    // Insert test events that will create correlations
    
    // 1. Travel + Cleaning conflict
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('trip1', 3, 'Beach Vacation', 'Family trip to the coast', ?, ?, 'travel')",
        &[&three_days.timestamp(), &(three_days + Duration::days(3)).timestamp()]
    )?;
    
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('clean1', 2, 'Cleaning Crew', 'Weekly house cleaning service', ?, ?, 'maintenance')",
        &[&four_days.timestamp(), &(four_days + Duration::hours(3)).timestamp()]
    )?;
    
    // 2. Important meeting with busy day before
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('presentation1', 1, 'Project Presentation', 'Important quarterly review presentation', ?, ?, 'meeting')",
        &[&five_days.timestamp(), &(five_days + Duration::hours(2)).timestamp()]
    )?;
    
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('meeting1', 1, 'Late Strategy Meeting', 'End of day strategy session', ?, ?, 'meeting')",
        &[&(five_days - Duration::hours(8)).timestamp(), &(five_days - Duration::hours(6)).timestamp()]
    )?;
    
    // 3. Overcommitted day
    let busy_day = tomorrow;
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('busy1', 1, 'Morning Standup', 'Daily team meeting', ?, ?, 'meeting')",
        &[&busy_day.timestamp(), &(busy_day + Duration::hours(1)).timestamp()]
    )?;
    
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('busy2', 1, 'Client Call', 'Important client check-in', ?, ?, 'meeting')",
        &[&(busy_day + Duration::hours(2)).timestamp(), &(busy_day + Duration::hours(3)).timestamp()]
    )?;
    
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('busy3', 1, 'Code Review', 'Team code review session', ?, ?, 'meeting')",
        &[&(busy_day + Duration::hours(4)).timestamp(), &(busy_day + Duration::hours(5)).timestamp()]
    )?;
    
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('busy4', 1, 'Design Workshop', 'UX design collaboration', ?, ?, 'meeting')",
        &[&(busy_day + Duration::hours(6)).timestamp(), &(busy_day + Duration::hours(8)).timestamp()]
    )?;
    
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('busy5', 1, 'Evening Retrospective', 'Sprint retrospective meeting', ?, ?, 'meeting')",
        &[&(busy_day + Duration::hours(9)).timestamp(), &(busy_day + Duration::hours(10)).timestamp()]
    )?;
    
    // 4. Another travel scenario
    database.execute_sql(
        "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
         VALUES ('delivery1', 2, 'Package Delivery', 'Important package expected', ?, ?, 'maintenance')",
        &[&day_after.timestamp(), &(day_after + Duration::hours(1)).timestamp()]
    )?;
    
    println!("✅ Inserted test events:");
    println!("   - Beach vacation (family calendar) with cleaning crew conflict");
    println!("   - Important presentation with busy preparation day");  
    println!("   - Overcommitted day with 5 meetings");
    println!("   - Package delivery scenario");
    
    Ok(())
}