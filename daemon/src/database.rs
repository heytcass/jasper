use anyhow::{Result, Context};
use rusqlite::{Connection, params, OptionalExtension};
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashSet;
use parking_lot::Mutex;
use tracing::{info, debug, warn};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type Database = Arc<DatabaseInner>;

pub struct DatabaseInner {
    connection: Mutex<Connection>,
    db_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: i64,
    pub source_id: String,
    pub calendar_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub location: Option<String>,
    pub event_type: Option<String>,
    pub participants: Option<String>, // JSON
    pub raw_data_json: Option<String>,
    pub is_all_day: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    pub id: i64,
    pub account_id: i64,
    pub calendar_id: String,
    pub calendar_name: String,
    pub calendar_type: Option<String>,
    pub color: Option<String>,
    pub metadata: Option<String>, // JSON
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correlation {
    pub id: String,
    pub event_ids: Vec<i64>,
    pub insight: String,
    pub action_needed: String,
    pub urgency_score: i32,
    pub discovered_at: DateTime<Utc>,
    pub recommended_glyph: Option<String>, // AI-chosen Nerd Font glyph
}

#[derive(Debug, Clone)]
pub struct EnrichedEvent {
    pub event: Event,
    pub calendar_info: Option<Calendar>,
}

impl DatabaseInner {
    pub async fn new(db_path: &PathBuf) -> Result<Database> {
        // Ensure data directory exists
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .with_context(|| format!("Failed to create data directory: {:?}", parent))?;
        }

        let connection = Connection::open(db_path)
            .with_context(|| format!("Failed to open database: {:?}", db_path))?;
            
        // Configure connection for optimal performance and resilience
        Self::configure_connection(&connection)
            .context("Failed to configure initial database connection")?;

        let db = Arc::new(DatabaseInner {
            connection: Mutex::new(connection),
            db_path: db_path.clone(),
        });

        db.run_migrations()
            .context("Failed to run database migrations")?;
        info!("Database initialized at {:?}", db_path);

        Ok(db)
    }
    
    /// Configure a SQLite connection with optimal settings for performance and resilience
    fn configure_connection(connection: &Connection) -> Result<()> {
        connection.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = 1000000;
             PRAGMA foreign_keys = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA busy_timeout = 30000;
             PRAGMA wal_autocheckpoint = 1000;
             PRAGMA mmap_size = 268435456;
             PRAGMA optimize;"
        ).context("Failed to execute PRAGMA configuration statements")?;
        Ok(())
    }
    
    /// Recover from database connection issues by reopening the connection
    fn recover_connection(&self) -> Result<()> {
        warn!("Attempting to recover database connection");
        
        let new_connection = Connection::open(&self.db_path)
            .with_context(|| format!("Failed to reopen database: {:?}", self.db_path))?;
            
        // Configure the connection with optimized settings for resilience
        Self::configure_connection(&new_connection)
            .context("Failed to configure recovered database connection")?;
        
        let mut conn_guard = self.connection.lock();
        *conn_guard = new_connection;
        
        info!("Database connection recovered successfully");
        Ok(())
    }
    
    /// Execute a database operation with automatic retry on connection failure
    fn with_connection_retry<F, R>(&self, operation: F) -> Result<R>
    where
        F: Fn(&Connection) -> Result<R> + Copy,
    {
        // First attempt
        {
            let conn = self.connection.lock();
            match operation(&*conn) {
                Ok(result) => return Ok(result),
                Err(e) => {
                    // Check if this is a connection-related error
                    if self.is_connection_error(&e) {
                        warn!("Database connection error detected: {}", e);
                        drop(conn); // Release the mutex before recovery
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        
        // Attempt recovery
        self.recover_connection()?;
        
        // Retry the operation
        let conn = self.connection.lock();
        operation(&*conn)
    }
    
    /// Check if an error indicates a connection issue
    fn is_connection_error(&self, error: &anyhow::Error) -> bool {
        let error_msg = error.to_string().to_lowercase();
        
        // SQLite connection-related errors
        error_msg.contains("database is locked") ||
        error_msg.contains("disk i/o error") ||
        error_msg.contains("database disk image is malformed") ||
        error_msg.contains("not a database") ||
        error_msg.contains("database is busy") ||
        error_msg.contains("cannot open database") ||
        error_msg.contains("unable to open database file") ||
        error_msg.contains("sql logic error") ||
        error_msg.contains("database or disk is full") ||
        error_msg.contains("file is not a database") ||
        error_msg.contains("attempt to write a readonly database")
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.connection.lock();
        
        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        
        // Create accounts table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS accounts (
                id INTEGER PRIMARY KEY,
                service_name TEXT NOT NULL UNIQUE,
                user_identifier TEXT,
                encrypted_refresh_token BLOB NOT NULL,
                last_sync_timestamp INTEGER
            )",
            [],
        )?;

        // Create calendars table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS calendars (
                id INTEGER PRIMARY KEY,
                account_id INTEGER REFERENCES accounts(id),
                calendar_id TEXT NOT NULL,
                calendar_name TEXT NOT NULL,
                calendar_type TEXT,
                color TEXT,
                metadata TEXT
            )",
            [],
        )?;

        // Create events table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY,
                source_id TEXT NOT NULL,
                calendar_id INTEGER REFERENCES calendars(id),
                title TEXT,
                description TEXT,
                start_time INTEGER NOT NULL,
                end_time INTEGER,
                location TEXT,
                event_type TEXT,
                participants TEXT,
                raw_data_json TEXT,
                is_all_day INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Add is_all_day column if it doesn't exist (for existing databases)
        conn.execute(
            "ALTER TABLE events ADD COLUMN is_all_day INTEGER DEFAULT 0",
            [],
        ).ok(); // Ignore error if column already exists

        // Create indexes for events table to optimize time-based queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_start_time ON events(start_time)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_calendar_start_time ON events(calendar_id, start_time)",
            [],
        )?;
        
        // Additional indexes for common query patterns identified by senior code review
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_time_range ON events(start_time, end_time)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_source_id ON events(source_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_events_end_time ON events(end_time)",
            [],
        )?;

        // Create tasks table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY,
                source_id TEXT NOT NULL,
                account_id INTEGER REFERENCES accounts(id),
                title TEXT,
                description TEXT,
                due_date INTEGER,
                priority INTEGER,
                project TEXT,
                tags TEXT,
                completed BOOLEAN DEFAULT FALSE,
                raw_data_json TEXT
            )",
            [],
        )?;

        // Indexes for tasks table to optimize common queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_due_date ON tasks(due_date)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks(priority)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_account_id ON tasks(account_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tasks_source_id ON tasks(source_id)",
            [],
        )?;

        // Create event_relationships table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS event_relationships (
                id INTEGER PRIMARY KEY,
                event1_id INTEGER REFERENCES events(id),
                event2_id INTEGER REFERENCES events(id),
                relationship_type TEXT,
                discovered_at INTEGER,
                confidence_score REAL,
                user_confirmed BOOLEAN DEFAULT NULL,
                notes TEXT
            )",
            [],
        )?;

        // Indexes for event_relationships table
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_relationships_event1 ON event_relationships(event1_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_relationships_event2 ON event_relationships(event2_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_event_relationships_type ON event_relationships(relationship_type)",
            [],
        )?;

        // Create user_patterns table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS user_patterns (
                id INTEGER PRIMARY KEY,
                pattern_type TEXT NOT NULL,
                pattern_data TEXT NOT NULL,
                occurrences INTEGER DEFAULT 1,
                last_seen INTEGER,
                confidence_score REAL
            )",
            [],
        )?;

        // Indexes for user_patterns table
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_user_patterns_type ON user_patterns(pattern_type)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_user_patterns_last_seen ON user_patterns(last_seen)",
            [],
        )?;
        
        // Additional indexes for calendars table
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_calendars_account_id ON calendars(account_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_calendars_calendar_id ON calendars(calendar_id)",
            [],
        )?;

        info!("Database migrations completed with comprehensive indexes");
        Ok(())
    }

    pub fn get_events_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Event>> {
        // Use pagination internally to limit memory usage
        self.get_events_in_range_paginated(start, end, None, None)
    }

    /// Get events in range with pagination support for large datasets
    pub fn get_events_in_range_paginated(&self, start: DateTime<Utc>, end: DateTime<Utc>, limit: Option<usize>, offset: Option<usize>) -> Result<Vec<Event>> {
        self.with_connection_retry(|conn| {
            let base_query = "SELECT id, source_id, calendar_id, title, description, start_time, end_time, 
                                    location, event_type, participants, raw_data_json, is_all_day
                             FROM events 
                             WHERE start_time >= ? AND start_time <= ?
                             ORDER BY start_time";
            
            let query = match (limit, offset) {
                (Some(limit), Some(offset)) => format!("{} LIMIT {} OFFSET {}", base_query, limit, offset),
                (Some(limit), None) => format!("{} LIMIT {}", base_query, limit),
                (None, Some(offset)) => format!("{} OFFSET {}", base_query, offset),
                (None, None) => {
                    // Default limit to prevent excessive memory usage
                    format!("{} LIMIT 10000", base_query)
                }
            };

            let mut stmt = conn.prepare(&query)?;

            let events = stmt.query_map(
                params![start.timestamp(), end.timestamp()],
                |row| {
                    Ok(Event {
                        id: row.get(0)?,
                        source_id: row.get(1)?,
                        calendar_id: row.get(2)?,
                        title: row.get(3)?,
                        description: row.get(4)?,
                        start_time: row.get(5)?,
                        end_time: row.get(6)?,
                        location: row.get(7)?,
                        event_type: row.get(8)?,
                        participants: row.get(9)?,
                        raw_data_json: row.get(10)?,
                        is_all_day: row.get::<_, Option<i32>>(11)?.map(|v| v != 0),
                    })
                }
            )?
            .collect::<Result<Vec<_>, _>>()?;

            Ok(events)
        })
    }

    /// Process events in batches to avoid loading large datasets into memory
    pub fn process_events_in_batches<F>(&self, start: DateTime<Utc>, end: DateTime<Utc>, batch_size: usize, mut processor: F) -> Result<()>
    where
        F: FnMut(&[Event]) -> Result<()>,
    {
        const DEFAULT_BATCH_SIZE: usize = 1000;
        let batch_size = if batch_size == 0 { DEFAULT_BATCH_SIZE } else { batch_size };
        
        let mut offset = 0;
        
        loop {
            let events = self.get_events_in_range_paginated(start, end, Some(batch_size), Some(offset))?;
            
            if events.is_empty() {
                break; // No more events to process
            }
            
            // Process this batch
            processor(&events)?;
            
            // If we got fewer events than the batch size, we're at the end
            if events.len() < batch_size {
                break;
            }
            
            offset += batch_size;
        }
        
        Ok(())
    }

    /// Count total events in range without loading them into memory
    pub fn count_events_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<i64> {
        self.with_connection_retry(|conn| {
            let mut stmt = conn.prepare(
                "SELECT COUNT(*) FROM events 
                 WHERE start_time >= ? AND start_time <= ?"
            )?;
            
            let count: i64 = stmt.query_row(
                params![start.timestamp(), end.timestamp()],
                |row| Ok(row.get(0)?)
            )?;
            
            Ok(count)
        })
    }

    pub fn create_event(&self, event: &Event) -> Result<i64> {
        self.with_connection_retry(|conn| {
            let _result = conn.execute(
                "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time,
                                    location, event_type, participants, raw_data_json, is_all_day)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    event.source_id,
                    event.calendar_id,
                    event.title,
                    event.description,
                    event.start_time,
                    event.end_time,
                    event.location,
                    event.event_type,
                    event.participants,
                    event.raw_data_json,
                    event.is_all_day.map(|v| if v { 1 } else { 0 }),
                ],
            )?;
            
            Ok(conn.last_insert_rowid())
        })
    }

    /// Bulk create events with deduplication check and transaction handling
    pub fn create_events_bulk(&self, events: &[Event]) -> Result<Vec<i64>> {
        self.with_connection_retry(|conn| {
            let mut event_ids = Vec::with_capacity(events.len());
            
            // Use a transaction for bulk operations
            let tx = conn.unchecked_transaction()?;
            
            {
                // Prepare statements for better performance (scoped to drop before commit)
                let mut insert_stmt = tx.prepare(
                    "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time,
                                        location, event_type, participants, raw_data_json, is_all_day)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                )?;
                
                let mut check_stmt = tx.prepare(
                    "SELECT id FROM events WHERE source_id = ?"
                )?;
                
                for event in events {
                    // Check if event already exists
                    let existing: Option<i64> = check_stmt.query_row(
                        params![event.source_id],
                        |row| Ok(row.get(0)?)
                    ).optional()?;
                    
                    if existing.is_some() {
                        debug!("Event {} already exists during bulk insert, skipping", event.source_id);
                        continue;
                    }
                    
                    // Insert new event
                    insert_stmt.execute(params![
                        event.source_id,
                        event.calendar_id,
                        event.title,
                        event.description,
                        event.start_time,
                        event.end_time,
                        event.location,
                        event.event_type,
                        event.participants,
                        event.raw_data_json,
                        event.is_all_day.map(|v| if v { 1 } else { 0 }),
                    ])?;
                    
                    event_ids.push(tx.last_insert_rowid());
                }
            }
            
            // Commit the transaction (statements are dropped, so no borrow issue)
            tx.commit()?;
            
            Ok(event_ids)
        })
    }

    /// Check which events already exist by source_id (for pre-filtering)
    pub fn get_existing_source_ids(&self, source_ids: &[String]) -> Result<HashSet<String>> {
        self.with_connection_retry(|conn| {
            let mut existing = HashSet::new();
            
            if source_ids.is_empty() {
                return Ok(existing);
            }
            
            // Use safe batch processing instead of dynamic SQL construction
            // Process in chunks to avoid hitting SQLite parameter limits
            const BATCH_SIZE: usize = 999; // SQLite default parameter limit is 999
            
            for chunk in source_ids.chunks(BATCH_SIZE) {
                // Build parameterized query with exact number of placeholders
                let placeholders = "?,".repeat(chunk.len());
                let placeholders = &placeholders[..placeholders.len()-1]; // Remove trailing comma
                let query = format!("SELECT source_id FROM events WHERE source_id IN ({})", placeholders);
                
                let mut stmt = conn.prepare(&query)?;
                let params: Vec<&dyn rusqlite::ToSql> = chunk.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                
                let rows = stmt.query_map(&params[..], |row| {
                    Ok(row.get::<_, String>(0)?)
                })?;
                
                for row in rows {
                    existing.insert(row?);
                }
            }
            
            Ok(existing)
        })
    }

    pub fn get_event_by_source_id(&self, source_id: &str) -> Result<Option<Event>> {
        let conn = self.connection.lock();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, calendar_id, title, description, start_time, end_time,
                    location, event_type, participants, raw_data_json, is_all_day
             FROM events 
             WHERE source_id = ?"
        )?;

        let mut rows = stmt.query_map(params![source_id], |row| {
            Ok(Event {
                id: row.get(0)?,
                source_id: row.get(1)?,
                calendar_id: row.get(2)?,
                title: row.get(3)?,
                description: row.get(4)?,
                start_time: row.get(5)?,
                end_time: row.get(6)?,
                location: row.get(7)?,
                event_type: row.get(8)?,
                participants: row.get(9)?,
                raw_data_json: row.get(10)?,
                is_all_day: row.get::<_, Option<i32>>(11)?.map(|v| v != 0),
            })
        })?;

        if let Some(event) = rows.next() {
            Ok(Some(event?))
        } else {
            Ok(None)
        }
    }



    
    /// Delete test events from the database (for cleanup operations)
    pub fn delete_test_events(&self) -> Result<usize> {
        let conn = self.connection.lock();
        let count = conn.execute(
            "DELETE FROM events WHERE calendar_id IN (
                SELECT id FROM calendars WHERE account_id IN (
                    SELECT id FROM accounts WHERE service_name = ?
                )
            )", 
            [&"test" as &dyn rusqlite::ToSql]
        )?;
        Ok(count)
    }
    
    /// Delete test calendars from the database (for cleanup operations)
    pub fn delete_test_calendars(&self) -> Result<usize> {
        let conn = self.connection.lock();
        let count = conn.execute(
            "DELETE FROM calendars WHERE account_id IN (
                SELECT id FROM accounts WHERE service_name = ?
            )", 
            [&"test" as &dyn rusqlite::ToSql]
        )?;
        Ok(count)
    }
    
    /// Delete test accounts from the database (for cleanup operations)
    pub fn delete_test_accounts(&self) -> Result<usize> {
        let conn = self.connection.lock();
        let count = conn.execute(
            "DELETE FROM accounts WHERE service_name = ?", 
            [&"test" as &dyn rusqlite::ToSql]
        )?;
        Ok(count)
    }
    
    /// Insert test account (for testing purposes only)
    pub fn insert_test_account(&self, timestamp: i64) -> Result<()> {
        let conn = self.connection.lock();
        conn.execute(
            "INSERT OR REPLACE INTO accounts (id, service_name, user_identifier, encrypted_refresh_token, last_sync_timestamp)
             VALUES (1, ?, ?, ?, ?)",
            [&"test" as &dyn rusqlite::ToSql, &"test_user", &"dummy_token", &timestamp]
        )?;
        Ok(())
    }
    
    /// Insert test calendar (for testing purposes only) 
    pub fn insert_test_calendar(&self, id: i64, calendar_id: &str, name: &str, calendar_type: &str, color: &str) -> Result<()> {
        let conn = self.connection.lock();
        conn.execute(
            "INSERT OR REPLACE INTO calendars (id, account_id, calendar_id, calendar_name, calendar_type, color)
             VALUES (?, 1, ?, ?, ?, ?)",
            [&id as &dyn rusqlite::ToSql, &calendar_id, &name, &calendar_type, &color]  
        )?;
        Ok(())
    }
    
    /// Insert test event (for testing purposes only)
    pub fn insert_test_event(&self, source_id: &str, calendar_id: i64, title: &str, description: &str, 
                           start_time: i64, end_time: i64, event_type: &str) -> Result<()> {
        let conn = self.connection.lock();
        conn.execute(
            "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time, event_type)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            [&source_id as &dyn rusqlite::ToSql, &calendar_id, &title, &description, 
             &start_time, &end_time, &event_type]
        )?;
        Ok(())
    }

    /// Create or update calendar record
    pub fn create_or_update_calendar(&self, calendar_id: &str, calendar_name: &str, calendar_type: Option<&str>) -> Result<i64> {
        let conn = self.connection.lock();
        
        // First, ensure we have an account record for Google Calendar
        let account_id = self.ensure_google_account(&conn)?;
        
        // Try to find existing calendar
        let existing_id: Option<i64> = conn.query_row(
            "SELECT id FROM calendars WHERE calendar_id = ? AND account_id = ?",
            params![calendar_id, account_id],
            |row| Ok(row.get(0)?)
        ).optional()?;
        
        if let Some(id) = existing_id {
            // Update existing calendar
            conn.execute(
                "UPDATE calendars SET calendar_name = ?, calendar_type = ? WHERE id = ?",
                params![calendar_name, calendar_type, id]
            )?;
            Ok(id)
        } else {
            // Create new calendar
            conn.execute(
                "INSERT INTO calendars (account_id, calendar_id, calendar_name, calendar_type, color)
                 VALUES (?, ?, ?, ?, ?)",
                params![account_id, calendar_id, calendar_name, calendar_type, Self::infer_calendar_color(calendar_id)]
            )?;
            Ok(conn.last_insert_rowid())
        }
    }
    

    /// Get calendar information by internal ID
    pub fn get_calendar_info(&self, calendar_id: i64) -> Result<Option<Calendar>> {
        let conn = self.connection.lock();
        let calendar = conn.query_row(
            "SELECT id, account_id, calendar_id, calendar_name, calendar_type, color, metadata FROM calendars WHERE id = ?",
            params![calendar_id],
            |row| Ok(Calendar {
                id: row.get(0)?,
                account_id: row.get(1)?,
                calendar_id: row.get(2)?,
                calendar_name: row.get(3)?,
                calendar_type: row.get(4)?,
                color: row.get(5)?,
                metadata: row.get(6)?,
            })
        ).optional()?;
        Ok(calendar)
    }
    
    /// Ensure Google account record exists
    fn ensure_google_account(&self, conn: &rusqlite::Connection) -> Result<i64> {
        // Try to find existing Google account
        let existing_id: Option<i64> = conn.query_row(
            "SELECT id FROM accounts WHERE service_name = 'google'",
            [],
            |row| Ok(row.get(0)?)
        ).optional()?;
        
        if let Some(id) = existing_id {
            Ok(id)
        } else {
            // Create new Google account record
            conn.execute(
                "INSERT INTO accounts (service_name, user_identifier, encrypted_refresh_token, last_sync_timestamp)
                 VALUES ('google', 'authenticated_user', 'stored_in_token_file', ?)",
                params![chrono::Utc::now().timestamp()]
            )?;
            Ok(conn.last_insert_rowid())
        }
    }
    
    /// Infer calendar color from ID patterns
    fn infer_calendar_color(calendar_id: &str) -> &'static str {
        match calendar_id {
            id if id == "primary" => "#4285F4", // Blue
            id if id.contains("family") => "#0F9D58", // Green  
            id if id.contains("house") || id.contains("home") => "#F4B400", // Yellow
            id if id.contains("work") || id.contains("office") => "#DB4437", // Red
            id if id.contains("holiday") => "#9C27B0", // Purple
            _ => "#757575", // Grey for unknown
        }
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use std::collections::HashSet;
    
    async fn create_test_database() -> Database {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join(format!("test_{}.db", uuid::Uuid::new_v4()));
        DatabaseInner::new(&db_path).await.expect("Failed to create test database")
    }
    
    #[tokio::test]
    async fn test_sql_injection_protection_get_existing_source_ids() {
        let database = create_test_database().await;
        
        // Insert some test data
        database.insert_test_account(chrono::Utc::now().timestamp()).unwrap();
        database.insert_test_calendar(1, "test", "Test Calendar", "test", "#FF0000").unwrap();
        database.insert_test_event("normal_event", 1, "Normal Event", "Description", 
                                 chrono::Utc::now().timestamp(), 
                                 chrono::Utc::now().timestamp() + 3600, "meeting").unwrap();
        
        // Test with malicious SQL injection attempts
        let malicious_inputs = vec![
            "'; DROP TABLE events; --",
            "' OR 1=1 --",
            "' UNION SELECT * FROM events --", 
            "'; DELETE FROM events WHERE 1=1; --",
            "' OR 'x'='x",
            "test'; INSERT INTO events (source_id, calendar_id, title) VALUES ('malicious', 1, 'hacked'); --",
        ];
        
        for malicious_input in malicious_inputs {
            let source_ids = vec![malicious_input.to_string()];
            
            // This should not cause SQL injection and should return safely
            let result = database.get_existing_source_ids(&source_ids);
            
            // The function should either:
            // 1. Return an empty set (no matching records)
            // 2. Return an error (but not crash)
            // 3. Never execute the malicious SQL
            match result {
                Ok(existing) => {
                    // Should be empty since malicious input won't match real source_ids
                    assert!(existing.is_empty(), 
                           "Malicious input '{}' should not match any records", malicious_input);
                }
                Err(_) => {
                    // Errors are acceptable as long as no injection occurred
                    // The important thing is that we don't crash or execute malicious SQL
                }
            }
        }
        
        // Verify our normal data is still intact (not dropped by injection)
        let normal_ids = vec!["normal_event".to_string()];
        let existing = database.get_existing_source_ids(&normal_ids).unwrap();
        assert_eq!(existing.len(), 1);
        assert!(existing.contains("normal_event"));
    }
    
    #[tokio::test]
    async fn test_parameterized_queries_prevent_injection() {
        let database = create_test_database().await;
        
        // Set up test data
        database.insert_test_account(chrono::Utc::now().timestamp()).unwrap();
        database.insert_test_calendar(1, "test", "Test Calendar", "test", "#FF0000").unwrap();
        
        // Test that our safe methods properly escape parameters
        let malicious_title = "'; DROP TABLE events; --";
        let malicious_description = "' OR 1=1; DELETE FROM calendars; --";
        
        // This should safely insert the malicious strings as literal text
        let result = database.insert_test_event(
            "safe_test", 1, malicious_title, malicious_description,
            chrono::Utc::now().timestamp(),
            chrono::Utc::now().timestamp() + 3600,
            "meeting"
        );
        
        assert!(result.is_ok(), "Parameterized insert should succeed");
        
        // Verify the malicious strings were stored as literal text (not executed as SQL)
        let events = database.get_events_in_range(
            chrono::Utc::now() - chrono::Duration::hours(1),
            chrono::Utc::now() + chrono::Duration::hours(2)
        ).unwrap();
        
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title.as_ref().unwrap(), malicious_title);
        assert_eq!(events[0].description.as_ref().unwrap(), malicious_description);
    }
    
    #[tokio::test]
    async fn test_batch_processing_limits() {
        let database = create_test_database().await;
        
        // Test with a very large number of source IDs to ensure we don't hit SQL limits
        let large_source_ids: Vec<String> = (0..2000)
            .map(|i| format!("source_{}", i))
            .collect();
        
        // This should handle the large batch safely without hitting SQLite parameter limits
        let result = database.get_existing_source_ids(&large_source_ids);
        assert!(result.is_ok(), "Large batch should be handled safely");
        
        let existing = result.unwrap();
        assert!(existing.is_empty(), "No existing records should be found");
    }
    
    #[tokio::test]
    async fn test_safe_cleanup_methods() {
        let database = create_test_database().await;
        
        // Set up test data
        database.insert_test_account(chrono::Utc::now().timestamp()).unwrap();
        database.insert_test_calendar(1, "test", "Test Calendar", "test", "#FF0000").unwrap();
        database.insert_test_event("test_event", 1, "Test", "Description",  
                                 chrono::Utc::now().timestamp(),
                                 chrono::Utc::now().timestamp() + 3600, "meeting").unwrap();
        
        // Verify data exists
        let events_before = database.get_events_in_range(
            chrono::Utc::now() - chrono::Duration::hours(1),
            chrono::Utc::now() + chrono::Duration::hours(2)
        ).unwrap();
        assert_eq!(events_before.len(), 1);
        
        // Test safe cleanup methods
        let events_deleted = database.delete_test_events().unwrap();
        assert_eq!(events_deleted, 1);
        
        let calendars_deleted = database.delete_test_calendars().unwrap();  
        assert_eq!(calendars_deleted, 1);
        
        let accounts_deleted = database.delete_test_accounts().unwrap();
        assert_eq!(accounts_deleted, 1);
        
        // Verify cleanup worked
        let events_after = database.get_events_in_range(
            chrono::Utc::now() - chrono::Duration::hours(1), 
            chrono::Utc::now() + chrono::Duration::hours(2)
        ).unwrap();
        assert_eq!(events_after.len(), 0);
    }
}