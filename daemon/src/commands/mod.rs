use anyhow::Result;
use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::config::Config;
use crate::database::Database;
use crate::correlation_engine::CorrelationEngine;

pub mod auth;
pub mod calendar;
pub mod daemon_ops;
pub mod waybar;

/// Trait for all command implementations
#[async_trait]
pub trait Command {
    /// Execute the command with the provided context
    async fn execute(&mut self, context: &CommandContext) -> Result<()>;
}

/// Shared context for all commands
pub struct CommandContext {
    pub config: Arc<RwLock<Config>>,
    pub database: Database,
    pub correlation_engine: CorrelationEngine,
    pub debug: bool,
    pub test_mode: bool,
}

impl CommandContext {
    pub fn new(
        config: Arc<RwLock<Config>>,
        database: Database,
        correlation_engine: CorrelationEngine,
        debug: bool,
        test_mode: bool,
    ) -> Self {
        Self {
            config,
            database,
            correlation_engine,
            debug,
            test_mode,
        }
    }
}