use anyhow::{Result, Context};
use rusqlite::{Connection, params, OptionalExtension};
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashSet;
use parking_lot::Mutex;
use tracing::{info, debug};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type Database = Arc<DatabaseInner>;

pub struct DatabaseInner {
    connection: Mutex<Connection>,
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
pub struct Insight {
    pub id: i64,
    pub timestamp: i64,
    pub insight_type: String,
    pub urgency_score: Option<i32>,
    pub full_insight_json: String,
    pub short_summary: String,
    pub related_events: Option<String>, // JSON array
    pub related_tasks: Option<String>,  // JSON array
    pub acknowledged: bool,
    pub user_feedback: Option<String>,
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

        let db = Arc::new(DatabaseInner {
            connection: Mutex::new(connection),
        });

        db.run_migrations().await?;
        info!("Database initialized at {:?}", db_path);

        Ok(db)
    }

    async fn run_migrations(&self) -> Result<()> {
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

        // Create insights table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS insights (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                insight_type TEXT NOT NULL,
                urgency_score INTEGER,
                full_insight_json TEXT NOT NULL,
                short_summary TEXT NOT NULL,
                related_events TEXT,
                related_tasks TEXT,
                acknowledged BOOLEAN DEFAULT FALSE,
                user_feedback TEXT
            )",
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

        info!("Database migrations completed");
        Ok(())
    }

    pub fn get_events_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Event>> {
        let conn = self.connection.lock();
        let mut stmt = conn.prepare(
            "SELECT id, source_id, calendar_id, title, description, start_time, end_time, 
                    location, event_type, participants, raw_data_json, is_all_day
             FROM events 
             WHERE start_time >= ? AND start_time <= ?
             ORDER BY start_time"
        )?;

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
    }

    pub async fn create_event(&self, event: &Event) -> Result<i64> {
        let conn = self.connection.lock();
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
    }

    /// Bulk create events with deduplication check and transaction handling
    pub async fn create_events_bulk(&self, events: &[Event]) -> Result<Vec<i64>> {
        let conn = self.connection.lock();
        let mut event_ids = Vec::with_capacity(events.len());
        
        // Use a transaction for bulk operations
        let tx = conn.unchecked_transaction()?;
        
        {
            // Prepare statements for better performance (scoped to drop before commit)
            let mut insert_stmt = tx.prepare(
                "INSERT INTO events (source_id, calendar_id, title, description, start_time, end_time,
                                    location, event_type, participants, raw_data_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
                ])?;
                
                event_ids.push(tx.last_insert_rowid());
            }
        }
        
        // Commit the transaction (statements are dropped, so no borrow issue)
        tx.commit()?;
        
        Ok(event_ids)
    }

    /// Check which events already exist by source_id (for pre-filtering)
    pub fn get_existing_source_ids(&self, source_ids: &[String]) -> Result<HashSet<String>> {
        let conn = self.connection.lock();
        let mut existing = HashSet::new();
        
        if source_ids.is_empty() {
            return Ok(existing);
        }
        
        // Build parameterized query for bulk lookup
        let placeholders: String = (0..source_ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("SELECT source_id FROM events WHERE source_id IN ({})", placeholders);
        
        let mut stmt = conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = source_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        
        let rows = stmt.query_map(&params[..], |row| {
            Ok(row.get::<_, String>(0)?)
        })?;
        
        for row in rows {
            existing.insert(row?);
        }
        
        Ok(existing)
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

    #[allow(dead_code)] // Used for storing AI insights - will be used when insight storage is implemented
    pub fn store_insight(&self, insight: &Insight) -> Result<i64> {
        let conn = self.connection.lock();
        conn.execute(
            "INSERT INTO insights (timestamp, insight_type, urgency_score, full_insight_json,
                                  short_summary, related_events, related_tasks, acknowledged, user_feedback)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                insight.timestamp,
                insight.insight_type,
                insight.urgency_score,
                insight.full_insight_json,
                insight.short_summary,
                insight.related_events,
                insight.related_tasks,
                insight.acknowledged,
                insight.user_feedback
            ]
        )?;

        Ok(conn.last_insert_rowid())
    }


    #[allow(dead_code)] // Used for acknowledging AI insights - will be used when insight management is implemented  
    pub fn acknowledge_insight(&self, insight_id: i64) -> Result<()> {
        let conn = self.connection.lock();
        conn.execute(
            "UPDATE insights SET acknowledged = TRUE WHERE id = ?",
            params![insight_id]
        )?;
        Ok(())
    }
    
    pub fn execute_sql(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<usize> {
        let conn = self.connection.lock();
        Ok(conn.execute(sql, params)?)
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