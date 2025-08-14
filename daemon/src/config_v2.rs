use std::sync::{Arc, OnceLock};
use arc_swap::ArcSwap;
use anyhow::Result;
use crate::config::Config;

/// Static configuration holder - initialized once at startup
static CONFIG: OnceLock<ArcSwap<Config>> = OnceLock::new();

/// Get the current configuration
/// This is lock-free and wait-free - no contention ever
pub fn config() -> Arc<Config> {
    CONFIG.get()
        .expect("Config not initialized - call init_config() first")
        .load_full()
}

/// Reload configuration from disk (for hot reload support)
pub async fn reload_config() -> Result<()> {
    let new_config_wrapped = Config::load().await?;
    let new_config = Arc::new(new_config_wrapped.read().clone());
    
    if let Some(swap) = CONFIG.get() {
        swap.store(new_config);
    } else {
        return Err(anyhow::anyhow!("Config not initialized"));
    }
    
    Ok(())
}

/// Initialize configuration (call once at startup)
/// This ensures CONFIG is initialized before any access
pub async fn init_config() -> Result<()> {
    let config_wrapped = Config::load().await?;
    let config = Arc::new(config_wrapped.read().clone());
    
    CONFIG.set(ArcSwap::from(config))
        .map_err(|_| anyhow::anyhow!("Config already initialized"))?;
    
    Ok(())
}

/// Configuration wrapper for gradual migration
/// This allows us to pass &Config instead of Arc<RwLock<Config>>
pub struct ConfigRef {
    inner: Arc<Config>,
}

impl ConfigRef {
    pub fn new() -> Self {
        Self {
            inner: config(),
        }
    }
    
    pub fn get(&self) -> &Config {
        &self.inner
    }
    
    pub fn reload(&mut self) {
        self.inner = config();
    }
}

impl std::ops::Deref for ConfigRef {
    type Target = Config;
    
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_config_access() {
        // Initialize config
        init_config().await.unwrap();
        
        // Multiple readers, no locks
        let config1 = config();
        let config2 = config();
        
        // Both point to same config
        assert!(Arc::ptr_eq(&config1, &config2));
    }
    
    #[tokio::test]
    async fn test_config_reload() {
        init_config().await.unwrap();
        
        let old_config = config();
        
        // Reload config
        reload_config().await.unwrap();
        
        let new_config = config();
        
        // Config has been swapped
        assert!(!Arc::ptr_eq(&old_config, &new_config));
    }
}