use thiserror::Error;

/// Structured error types for the Jasper companion daemon
#[derive(Error, Debug, Clone)]
#[allow(dead_code)]
pub enum JasperError {
    /// Configuration errors
    #[error("Configuration error: {message}")]
    Config { message: String },
    
    /// Database operation errors
    #[error("Database error: {operation} failed: {message}")]
    Database {
        operation: String,
        message: String,
    },
    
    /// Calendar synchronization errors
    #[error("Calendar sync error: {message}")]
    CalendarSync { message: String },
    
    /// Authentication errors
    #[error("Authentication error: {service} authentication failed: {message}")]
    Authentication { service: String, message: String },
    
    /// API call errors (Claude AI, Google Calendar, etc.)
    #[error("API error: {service} API call failed: {message}")]
    Api { service: String, message: String },
    
    /// Network connectivity errors
    #[error("Network error: {message}")]
    Network { message: String },
    
    /// File system errors
    #[error("File system error: {operation} failed for path '{path}': {message}")]
    FileSystem {
        operation: String,
        path: String,
        message: String,
    },
    
    /// Parsing errors (JSON, TOML, etc.)
    #[error("Parsing error: Failed to parse {format}: {message}")]
    Parsing {
        format: String,
        message: String,
    },
    
    /// Timeout errors
    #[error("Timeout error: {operation} timed out after {timeout_seconds}s")]
    Timeout {
        operation: String,
        timeout_seconds: u64,
    },
    
    /// Validation errors
    #[error("Validation error: {field} is invalid: {message}")]
    Validation { field: String, message: String },
    
    /// Service unavailable errors
    #[error("Service unavailable: {service} is not configured or not available")]
    ServiceUnavailable { service: String },
    
    /// Internal errors that shouldn't happen
    #[error("Internal error: {message}")]
    Internal { message: String },
}

/// Result type alias using JasperError
pub type JasperResult<T> = std::result::Result<T, JasperError>;

/// Convert anyhow::Error to JasperError
impl From<anyhow::Error> for JasperError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal {
            message: error.to_string(),
        }
    }
}

/// Convert std::io::Error to JasperError
impl From<std::io::Error> for JasperError {
    fn from(error: std::io::Error) -> Self {
        Self::FileSystem {
            operation: "unknown".to_string(),
            path: "unknown".to_string(),
            message: error.to_string(),
        }
    }
}

/// Convert rusqlite::Error to JasperError
impl From<rusqlite::Error> for JasperError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database {
            operation: "sql_operation".to_string(),
            message: error.to_string(),
        }
    }
}

/// Convert serde_json::Error to JasperError  
impl From<serde_json::Error> for JasperError {
    fn from(error: serde_json::Error) -> Self {
        Self::Parsing {
            format: "JSON".to_string(),
            message: error.to_string(),
        }
    }
}

/// Convert toml::de::Error to JasperError
impl From<toml::de::Error> for JasperError {
    fn from(error: toml::de::Error) -> Self {
        Self::Parsing {
            format: "TOML".to_string(),
            message: error.to_string(),
        }
    }
}

/// Convert reqwest::Error to JasperError
impl From<reqwest::Error> for JasperError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout {
                operation: "HTTP request".to_string(),
                timeout_seconds: 30, // Default assumption
            }
        } else if error.is_connect() {
            Self::Network {
                message: format!("Connection failed: {}", error),
            }
        } else {
            Self::Api {
                service: "HTTP".to_string(),
                message: error.to_string(),
            }
        }
    }
}

/// Convert zbus::Error to JasperError
impl From<zbus::Error> for JasperError {
    fn from(error: zbus::Error) -> Self {
        Self::Internal {
            message: format!("D-Bus error: {}", error),
        }
    }
}