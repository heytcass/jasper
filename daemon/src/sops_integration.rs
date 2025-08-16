#![allow(dead_code)]

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::process::Command;
use std::path::PathBuf;
use std::env;
use tracing::{debug, warn, info};

/// SOPS integration for secure secret management
#[derive(Default)]
pub struct SopsSecrets {
    secrets: HashMap<String, String>,
}

impl SopsSecrets {
    /// Load secrets from SOPS encrypted file
    pub fn load() -> Result<Self> {
        Self::load_from_paths(&Self::get_default_paths())
    }
    
    /// Load secrets with custom search paths
    pub fn load_from_paths(paths: &[PathBuf]) -> Result<Self> {
        for path in paths {
            debug!("Trying SOPS secrets file: {:?}", path);
            if path.exists() {
                match Self::load_from_file_path(path) {
                    Ok(secrets) => {
                        info!("Successfully loaded secrets from: {:?}", path);
                        return Ok(secrets);
                    }
                    Err(e) => {
                        warn!("Failed to load secrets from {:?}: {}", path, e);
                        continue;
                    }
                }
            }
        }
        
        // Fallback to empty secrets if no file found
        warn!("No SOPS secrets file found in search paths: {:?}", paths);
        Ok(Self::default())
    }
    
    /// Get default search paths for SOPS secrets files
    fn get_default_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        
        // Check environment variable first
        if let Ok(custom_path) = env::var("JASPER_SOPS_PATH") {
            paths.push(PathBuf::from(custom_path));
        }
        
        // Add standard locations with proper path expansion
        if let Some(home_dir) = env::var("HOME").ok() {
            let home_path = PathBuf::from(home_dir);
            paths.push(home_path.join(".nixos/secrets/secrets.yaml"));
            paths.push(home_path.join(".config/jasper-companion/secrets.yaml"));
        }
        
        // System-wide location
        paths.push(PathBuf::from("/etc/jasper-companion/secrets.yaml"));
        
        // Development/local location
        if let Ok(current_dir) = env::current_dir() {
            paths.push(current_dir.join("secrets.yaml"));
            paths.push(current_dir.join(".secrets.yaml"));
        }
        
        paths
    }
    
    /// Load secrets from a specific SOPS file (string path for backward compatibility)
    pub fn load_from_file(path: &str) -> Result<Self> {
        Self::load_from_file_path(&PathBuf::from(path))
    }
    
    /// Load secrets from a specific SOPS file path
    fn load_from_file_path(path: &PathBuf) -> Result<Self> {
        let path_str = path.to_string_lossy();
        debug!("Loading secrets from SOPS file: {}", path_str);
        
        // Validate file exists and is readable
        if !path.exists() {
            return Err(anyhow!("SOPS file does not exist: {}", path_str));
        }
        
        if !path.is_file() {
            return Err(anyhow!("SOPS path is not a file: {}", path_str));
        }
        
        // Try different methods to run sops
        Self::decrypt_sops_file(path)
    }
    
    /// Decrypt SOPS file using different available methods
    fn decrypt_sops_file(path: &PathBuf) -> Result<Self> {
        let path_str = path.to_string_lossy();
        
        // Method 1: Try direct sops command (if available in PATH)
        if let Ok(output) = Command::new("sops")
            .arg("-d")
            .arg(&*path_str)
            .output() {
            
            if output.status.success() {
                let decrypted = String::from_utf8_lossy(&output.stdout);
                return Self::parse_yaml(&decrypted);
            } else {
                debug!("Direct sops command failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
        
        // Method 2: Try sops via nix-shell
        if let Ok(output) = Command::new("nix-shell")
            .arg("-p")
            .arg("sops")
            .arg("--run")
            .arg(&format!("sops -d {}", path_str))
            .output() {
            
            if output.status.success() {
                let decrypted = String::from_utf8_lossy(&output.stdout);
                return Self::parse_yaml(&decrypted);
            } else {
                debug!("Nix-shell sops command failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
        
        // Method 3: Try sops via nix develop (if in a flake)
        if let Ok(output) = Command::new("nix")
            .arg("develop")
            .arg("--command")
            .arg("sops")
            .arg("-d")
            .arg(&*path_str)
            .output() {
            
            if output.status.success() {
                let decrypted = String::from_utf8_lossy(&output.stdout);
                return Self::parse_yaml(&decrypted);
            } else {
                debug!("Nix develop sops command failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
        
        // All methods failed
        Err(anyhow!("Could not decrypt SOPS file using any available method: {}", path_str))
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