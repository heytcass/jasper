use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::process::Command;
use tracing::{debug, warn, info};

/// SOPS integration for secure secret management
#[derive(Default)]
pub struct SopsSecrets {
    secrets: HashMap<String, String>,
}

impl SopsSecrets {
    /// Load secrets from SOPS encrypted file
    pub fn load() -> Result<Self> {
        // Try standard locations for SOPS secrets
        let paths = [
            "~/.nixos/secrets/secrets.yaml",
            "~/.config/jasper-companion/secrets.yaml", 
            "/etc/jasper-companion/secrets.yaml"
        ];
        
        for path in &paths {
            if let Ok(secrets) = Self::load_from_file(path) {
                return Ok(secrets);
            }
        }
        
        // Fallback to empty secrets if no file found
        warn!("No SOPS secrets file found in standard locations");
        Ok(Self::default())
    }
    
    /// Load secrets from a specific SOPS file
    pub fn load_from_file(path: &str) -> Result<Self> {
        info!("Loading secrets from SOPS file: {}", path);
        
        // Run sops -d to decrypt the file using nix-shell
        let output = Command::new("nix-shell")
            .arg("-p")
            .arg("sops")
            .arg("--run")
            .arg(&format!("sops -d {}", path))
            .output();
        
        match output {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(anyhow!("SOPS decryption failed: {}", stderr));
                }
                
                let decrypted = String::from_utf8_lossy(&output.stdout);
                Self::parse_yaml(&decrypted)
            }
            Err(e) => {
                warn!("SOPS command failed: {}. Falling back to config file values.", e);
                // Return empty secrets map to fall back to config file values
                Ok(Self {
                    secrets: HashMap::new(),
                })
            }
        }
    }
    
    /// Parse YAML content into secrets map with support for nested structures
    fn parse_yaml(yaml_content: &str) -> Result<Self> {
        let mut secrets = HashMap::new();
        let mut current_context = Vec::new();
        
        for line in yaml_content.lines() {
            let line = line.trim_end();
            if line.trim().is_empty() || line.trim().starts_with('#') || line.trim().starts_with("sops:") {
                continue;
            }
            
            // Calculate indentation level
            let indent_level = line.len() - line.trim_start().len();
            let line_content = line.trim();
            
            if let Some(colon_pos) = line_content.find(':') {
                let key = line_content[..colon_pos].trim();
                let value = line_content[colon_pos + 1..].trim();
                
                // Adjust context based on indentation
                let expected_level = current_context.len() * 4; // Assuming 4 spaces per level
                if indent_level < expected_level {
                    // Pop context until we match indentation
                    while current_context.len() > indent_level / 4 {
                        current_context.pop();
                    }
                } else if indent_level == expected_level + 4 {
                    // We're one level deeper, context should already be set
                }
                
                if value.is_empty() {
                    // This is a section header, add to context
                    current_context.truncate(indent_level / 4);
                    current_context.push(key.to_string());
                } else {
                    // This is a key-value pair
                    let mut full_key = current_context.join(".");
                    if !full_key.is_empty() {
                        full_key.push('.');
                    }
                    full_key.push_str(key);
                    
                    // Remove quotes if present
                    let value = if value.starts_with('"') && value.ends_with('"') {
                        &value[1..value.len() - 1]
                    } else {
                        value
                    };
                    
                    // Skip sops metadata fields
                    if !full_key.starts_with("sops") && !full_key.contains("lastmodified") && !full_key.contains("mac") {
                        secrets.insert(full_key.clone(), value.to_string());
                        debug!("Loaded secret: {}", full_key);
                    }
                }
            }
        }
        
        info!("Successfully loaded {} secrets from SOPS", secrets.len());
        Ok(Self { secrets })
    }
    
    /// Get a secret by key
    pub fn get(&self, key: &str) -> Option<&String> {
        self.secrets.get(key)
    }
    
    /// Get a secret by key with fallback
    pub fn get_or_fallback(&self, key: &str, fallback: &str) -> String {
        match self.secrets.get(key) {
            Some(value) => {
                debug!("Using SOPS secret for key: {}", key);
                value.clone()
            }
            None => {
                debug!("Using fallback value for key: {}", key);
                fallback.to_string()
            }
        }
    }
    
    /// Check if a secret exists
    pub fn has(&self, key: &str) -> bool {
        self.secrets.contains_key(key)
    }
    
    /// Get all secret keys (for debugging)
    pub fn keys(&self) -> Vec<&String> {
        self.secrets.keys().collect()
    }
}

/// Helper function to securely load API keys
pub fn load_api_keys() -> Result<SopsSecrets> {
    SopsSecrets::load()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_yaml() {
        let yaml = r#"
# Test secrets
api_key: "test_key_123"
client_secret: "secret_456"
sops:
    lastmodified: "2023-01-01"
"#;
        
        let secrets = SopsSecrets::parse_yaml(yaml).unwrap();
        assert_eq!(secrets.get("api_key"), Some(&"test_key_123".to_string()));
        assert_eq!(secrets.get("client_secret"), Some(&"secret_456".to_string()));
        assert_eq!(secrets.get("sops"), None); // Should be filtered out
    }
}