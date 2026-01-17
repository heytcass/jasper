// Simplified calendar context source for new architecture
// TODO: Implement proper context source interface when needed

use std::sync::Arc;
use parking_lot::RwLock;
use crate::config::Config;
use crate::database::Database;

/// Simplified calendar context source - placeholder for now
pub struct CalendarContextSource {
    _config: Arc<RwLock<Config>>,
    _database: Database,
}

impl CalendarContextSource {
    /// Create a new calendar context source
    pub fn new(config: Arc<RwLock<Config>>, database: Database) -> Self {
        Self {
            _config: config,
            _database: database,
        }
    }
}