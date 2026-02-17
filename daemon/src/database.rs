use anyhow::Context;
use crate::errors::{JasperError, JasperResult};
use rusqlite::{Connection, params, OptionalExtension};
use std::path::PathBuf;
use std::sync::Arc;
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
pub struct Insight {
    pub id: i64,
    pub emoji: String,
    pub insight: String,
    pub context_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveFrontend {
    pub id: String,
    pub pid: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
}

impl DatabaseInner {
    pub async fn new(db_path: &PathBuf) -> JasperResult<Database> {
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
    fn configure_connection(connection: &Connection) -> JasperResult<()> {
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
    fn recover_connection(&self) -> JasperResult<()> {
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
    fn with_connection_retry<F, R>(&self, operation: F) -> JasperResult<R>
    where
        F: Fn(&Connection) -> JasperResult<R> + Copy,
    {
        // First attempt
        {
            let conn = self.connection.lock();
            match operation(&conn) {
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
        operation(&conn)
    }
    
    /// Check if an error indicates a connection issue
    fn is_connection_error(&self, error: &JasperError) -> bool {
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

    fn run_migrations(&self) -> JasperResult<()> {
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

        // Create insights table for the new simplified architecture
        conn.execute(
            "CREATE TABLE IF NOT EXISTS insights (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                emoji TEXT NOT NULL,
                insight TEXT NOT NULL,
                context_hash TEXT,
                created_at INTEGER DEFAULT (strftime('%s', 'now')),
                expires_at INTEGER,
                is_active INTEGER DEFAULT 1
            )",
            [],
        )?;

        // Indexes for insights table
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_insights_created_at ON insights(created_at)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_insights_active ON insights(is_active)",
            [],
        )?;

        // Create context_snapshots table to track what triggered each insight
        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                insight_id INTEGER REFERENCES insights(id) ON DELETE CASCADE,
                source TEXT NOT NULL,
                snapshot_json TEXT NOT NULL,
                significance_score REAL,
                created_at INTEGER DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;

        // Indexes for context_snapshots table
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_snapshots_insight_id ON context_snapshots(insight_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_context_snapshots_source ON context_snapshots(source)",
            [],
        )?;

        // Create active_frontends table to track which frontends are running
        conn.execute(
            "CREATE TABLE IF NOT EXISTS active_frontends (
                id TEXT PRIMARY KEY,
                pid INTEGER,
                started_at INTEGER DEFAULT (strftime('%s', 'now')),
                last_heartbeat INTEGER DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;

        // Index for active_frontends table
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_active_frontends_heartbeat ON active_frontends(last_heartbeat)",
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

    pub fn get_events_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> JasperResult<Vec<Event>> {
        // Use pagination internally to limit memory usage
        self.get_events_in_range_paginated(start, end, None, None)
    }

    /// Get events in range with pagination support for large datasets
    pub fn get_events_in_range_paginated(&self, start: DateTime<Utc>, end: DateTime<Utc>, limit: Option<usize>, offset: Option<usize>) -> JasperResult<Vec<Event>> {
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

    /// Bulk create events with deduplication check and transaction handling
    pub fn create_events_bulk(&self, events: &[Event]) -> JasperResult<Vec<i64>> {
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
                        |row| row.get(0)
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

    /// Delete all events for a given calendar database ID (used during sync refresh)
    pub fn delete_events_for_calendar(&self, calendar_db_id: i64) -> JasperResult<usize> {
        self.with_connection_retry(|conn| {
            let count = conn.execute(
                "DELETE FROM events WHERE calendar_id = ?",
                params![calendar_db_id],
            )?;
            Ok(count)
        })
    }

    /// Create or update calendar record
    pub fn create_or_update_calendar(&self, calendar_id: &str, calendar_name: &str, calendar_type: Option<&str>) -> JasperResult<i64> {
        let conn = self.connection.lock();
        
        // First, ensure we have an account record for Google Calendar
        let account_id = self.ensure_google_account(&conn)?;
        
        // Try to find existing calendar
        let existing_id: Option<i64> = conn.query_row(
            "SELECT id FROM calendars WHERE calendar_id = ? AND account_id = ?",
            params![calendar_id, account_id],
            |row| row.get(0)
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

    /// Ensure Google account record exists
    fn ensure_google_account(&self, conn: &rusqlite::Connection) -> JasperResult<i64> {
        // Try to find existing Google account
        let existing_id: Option<i64> = conn.query_row(
            "SELECT id FROM accounts WHERE service_name = 'google'",
            [],
            |row| row.get(0)
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
            "primary" => "#4285F4", // Blue
            id if id.contains("family") => "#0F9D58", // Green  
            id if id.contains("house") || id.contains("home") => "#F4B400", // Yellow
            id if id.contains("work") || id.contains("office") => "#DB4437", // Red
            id if id.contains("holiday") => "#9C27B0", // Purple
            _ => "#757575", // Grey for unknown
        }
    }

    /// Store a new insight from AI analysis
    pub fn store_insight(&self, emoji: &str, insight: &str, context_hash: Option<&str>) -> JasperResult<i64> {
        self.with_connection_retry(|conn| {
            conn.execute(
                "INSERT INTO insights (emoji, insight, context_hash) VALUES (?, ?, ?)",
                params![emoji, insight, context_hash],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    /// Get the latest active insight
    pub fn get_latest_insight(&self) -> JasperResult<Option<Insight>> {
        self.with_connection_retry(|conn| {
            let insight = conn.query_row(
                "SELECT id, emoji, insight, context_hash, created_at, expires_at, is_active
                 FROM insights 
                 WHERE is_active = 1 
                 ORDER BY created_at DESC 
                 LIMIT 1",
                [],
                |row| {
                    Ok(Insight {
                        id: row.get(0)?,
                        emoji: row.get(1)?,
                        insight: row.get(2)?,
                        context_hash: row.get(3)?,
                        created_at: DateTime::from_timestamp(row.get::<_, i64>(4)?, 0).unwrap_or_default(),
                        expires_at: row.get::<_, Option<i64>>(5)?
                            .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default()),
                        is_active: row.get::<_, i64>(6)? != 0,
                    })
                }
            ).optional()?;
            Ok(insight)
        })
    }

    /// Get insight by ID
    pub fn get_insight_by_id(&self, insight_id: i64) -> JasperResult<Option<Insight>> {
        self.with_connection_retry(|conn| {
            let insight = conn.query_row(
                "SELECT id, emoji, insight, context_hash, created_at, expires_at, is_active
                 FROM insights 
                 WHERE id = ?",
                params![insight_id],
                |row| {
                    Ok(Insight {
                        id: row.get(0)?,
                        emoji: row.get(1)?,
                        insight: row.get(2)?,
                        context_hash: row.get(3)?,
                        created_at: DateTime::from_timestamp(row.get::<_, i64>(4)?, 0).unwrap_or_default(),
                        expires_at: row.get::<_, Option<i64>>(5)?
                            .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default()),
                        is_active: row.get::<_, i64>(6)? != 0,
                    })
                }
            ).optional()?;
            Ok(insight)
        })
    }

    /// Get the N most recent insights (for deduplication in prompts)
    pub fn get_recent_insights(&self, limit: u32) -> JasperResult<Vec<Insight>> {
        self.with_connection_retry(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, emoji, insight, context_hash, created_at, expires_at, is_active
                 FROM insights
                 ORDER BY created_at DESC
                 LIMIT ?",
            )?;
            let insights = stmt.query_map(params![limit], |row| {
                Ok(Insight {
                    id: row.get(0)?,
                    emoji: row.get(1)?,
                    insight: row.get(2)?,
                    context_hash: row.get(3)?,
                    created_at: DateTime::from_timestamp(row.get::<_, i64>(4)?, 0).unwrap_or_default(),
                    expires_at: row.get::<_, Option<i64>>(5)?
                        .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default()),
                    is_active: row.get::<_, i64>(6)? != 0,
                })
            })?.collect::<Result<Vec<_>, _>>()?;
            Ok(insights)
        })
    }

    /// Store context snapshot that triggered an insight
    pub fn store_context_snapshot(&self, insight_id: i64, source: &str, snapshot_json: &str, significance_score: Option<f32>) -> JasperResult<i64> {
        self.with_connection_retry(|conn| {
            conn.execute(
                "INSERT INTO context_snapshots (insight_id, source, snapshot_json, significance_score) VALUES (?, ?, ?, ?)",
                params![insight_id, source, snapshot_json, significance_score],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    /// Register a frontend as active
    pub fn register_frontend(&self, frontend_id: &str, pid: Option<i32>) -> JasperResult<()> {
        self.with_connection_retry(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO active_frontends (id, pid) VALUES (?, ?)",
                params![frontend_id, pid],
            )?;
            Ok(())
        })
    }

    /// Unregister a frontend 
    pub fn unregister_frontend(&self, frontend_id: &str) -> JasperResult<()> {
        self.with_connection_retry(|conn| {
            conn.execute(
                "DELETE FROM active_frontends WHERE id = ?",
                params![frontend_id],
            )?;
            Ok(())
        })
    }

    /// Update frontend heartbeat
    pub fn update_frontend_heartbeat(&self, frontend_id: &str) -> JasperResult<()> {
        self.with_connection_retry(|conn| {
            conn.execute(
                "UPDATE active_frontends SET last_heartbeat = strftime('%s', 'now') WHERE id = ?",
                params![frontend_id],
            )?;
            Ok(())
        })
    }

    /// Get list of active frontends
    pub fn get_active_frontends(&self) -> JasperResult<Vec<ActiveFrontend>> {
        self.with_connection_retry(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, pid, started_at, last_heartbeat FROM active_frontends"
            )?;
            
            let frontends = stmt.query_map([], |row| {
                Ok(ActiveFrontend {
                    id: row.get(0)?,
                    pid: row.get(1)?,
                    started_at: DateTime::from_timestamp(row.get::<_, i64>(2)?, 0).unwrap_or_default(),
                    last_heartbeat: DateTime::from_timestamp(row.get::<_, i64>(3)?, 0).unwrap_or_default(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
            
            Ok(frontends)
        })
    }

    /// Clean up expired frontends (no heartbeat for > 60 seconds)
    pub fn cleanup_expired_frontends(&self) -> JasperResult<usize> {
        self.with_connection_retry(|conn| {
            let count = conn.execute(
                "DELETE FROM active_frontends WHERE last_heartbeat < strftime('%s', 'now') - 60",
                [],
            )?;
            Ok(count)
        })
    }

    /// Check if any frontends are currently active
    pub fn has_active_frontends(&self) -> JasperResult<bool> {
        // Clean up expired frontends first
        self.cleanup_expired_frontends()?;
        
        self.with_connection_retry(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM active_frontends",
                [],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
    }
}

