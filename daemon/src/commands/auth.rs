use anyhow::Result;
use async_trait::async_trait;
use tracing::info;
use std::io::{self, Write};

use crate::calendar_sync::CalendarSyncService;
use super::{Command, CommandContext};

/// Command to set up Google Calendar authentication
pub struct AuthGoogleCommand;

/// Command to set Claude API key
pub struct SetApiKeyCommand {
    pub api_key: String,
}

#[async_trait]
impl Command for AuthGoogleCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        info!("Setting up Google Calendar authentication...");
        
        // Check if Google Calendar is configured
        let gc_config = {
            let config_guard = context.config.read();
            config_guard.google_calendar.clone()
        };

        let gc_config = match gc_config {
            Some(config) if !config.client_id.is_empty() => config,
            _ => {
                println!("❌ Google Calendar not configured!");
                println!("Please add your Google Calendar OAuth2 credentials to the config file:");
                println!("  1. Go to https://console.developers.google.com/");
                println!("  2. Create a new project or select existing one");
                println!("  3. Enable Google Calendar API");
                println!("  4. Create OAuth2 credentials (Desktop application)");
                println!("  5. Add credentials to ~/.config/jasper-companion/config.toml:");
                println!("");
                println!("[google_calendar]");
                println!("enabled = true");
                println!("client_id = \"your-client-id.googleusercontent.com\"");
                println!("client_secret = \"your-client-secret\"");
                println!("redirect_uri = \"http://localhost:8080/auth/callback\"");
                println!("calendar_ids = [\"primary\"]");
                println!("sync_interval_minutes = 15");
                return Ok(());
            }
        };

        println!("✅ Google Calendar configuration found");

        let mut calendar_sync = CalendarSyncService::new(context.config.clone(), context.database.clone())?;
        
        if calendar_sync.is_authenticated().await {
            println!("✅ Already authenticated with Google Calendar");
            
            // List available calendars
            match calendar_sync.list_calendars().await {
                Ok(calendars) => {
                    println!("\n📅 Available calendars:");
                    for (id, name) in calendars {
                        let is_configured = gc_config.calendar_ids.contains(&id);
                        let status = if is_configured { "✓" } else { " " };
                        println!("  {} {} ({})", status, name, id);
                    }
                }
                Err(e) => {
                    println!("⚠️  Could not list calendars: {}", e);
                }
            }
            
            return Ok(());
        }

        println!("🔐 Google Calendar authentication required");
        
        // Get auth URL
        let (auth_url, csrf_token) = calendar_sync.get_auth_url()?
            .ok_or_else(|| anyhow::anyhow!("Failed to get authentication URL"))?;

        println!("\n📋 Follow these steps:");
        println!("1. Open this URL in your browser:");
        println!("   {}", auth_url);
        println!("\n2. Grant access to your Google Calendar");
        println!("3. Copy the authorization code from the redirect URL");
        println!("4. Paste it here when prompted");
        
        print!("\n🔑 Enter authorization code: ");
        io::stdout().flush()?;
        
        let mut auth_code = String::new();
        io::stdin().read_line(&mut auth_code)?;
        let auth_code = auth_code.trim();

        if auth_code.is_empty() {
            println!("❌ No authorization code provided");
            return Ok(());
        }

        println!("🔄 Exchanging authorization code for access token...");
        match calendar_sync.authenticate_with_code(auth_code, &csrf_token).await {
            Ok(()) => {
                println!("✅ Google Calendar authentication successful!");
                println!("🔄 Performing initial calendar sync...");
                
                // List calendars after authentication
                match calendar_sync.list_calendars().await {
                    Ok(calendars) => {
                        println!("\n📅 Your calendars:");
                        for (id, name) in calendars {
                            println!("   {} ({})", name, id);
                        }
                        println!("\n💡 You can configure which calendars to sync in the config file");
                    }
                    Err(e) => {
                        println!("⚠️  Could not list calendars: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("❌ Authentication failed: {}", e);
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Command for SetApiKeyCommand {
    async fn execute(&mut self, context: &CommandContext) -> Result<()> {
        println!("🔑 Setting Claude API key in configuration...");
        
        // Validate the API key format
        if !self.api_key.starts_with("sk-ant-") {
            return Err(anyhow::anyhow!("Invalid API key format. Claude API keys should start with 'sk-ant-'"));
        }
        
        // Update config
        {
            let mut config_guard = context.config.write();
            config_guard.ai.api_key = Some(self.api_key.clone());
        }
        
        // Save config to file
        let config_path = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("jasper-companion")
            .join("config.toml");
        
        // Ensure directory exists
        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        // Save config
        let config_content = {
            let config_guard = context.config.read();
            toml::to_string_pretty(&*config_guard)?
        };
        
        tokio::fs::write(&config_path, config_content).await?;
        
        println!("✅ API key saved to {}", config_path.display());
        println!("🚀 Jasper can now use Claude Sonnet 4 for AI analysis!");
        println!("💡 Restart the daemon to use the new API key");
        
        Ok(())
    }
}