use thiserror::Error;

/// Structured error types for the Jasper companion daemon
#[derive(Error, Debug, Clone)]
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

impl JasperError {
    /// Create a configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        Self::Config {
            message: message.into(),
        }
    }
    
    /// Create a database error
    pub fn database<S: Into<String>>(operation: S, message: S) -> Self {
        Self::Database {
            operation: operation.into(),
            message: message.into(),
        }
    }
    
    /// Create a calendar sync error
    pub fn calendar_sync<S: Into<String>>(message: S) -> Self {
        Self::CalendarSync {
            message: message.into(),
        }
    }
    
    /// Create an authentication error
    pub fn authentication<S: Into<String>>(service: S, message: S) -> Self {
        Self::Authentication {
            service: service.into(),
            message: message.into(),
        }
    }
    
    /// Create an API error
    pub fn api<S: Into<String>>(service: S, message: S) -> Self {
        Self::Api {
            service: service.into(),
            message: message.into(),
        }
    }
    
    /// Create a network error
    pub fn network<S: Into<String>>(message: S) -> Self {
        Self::Network {
            message: message.into(),
        }
    }
    
    /// Create a file system error
    pub fn file_system<S: Into<String>>(operation: S, path: S, message: S) -> Self {
        Self::FileSystem {
            operation: operation.into(),
            path: path.into(),
            message: message.into(),
        }
    }
    
    /// Create a parsing error
    pub fn parsing<S: Into<String>>(format: S, message: S) -> Self {
        Self::Parsing {
            format: format.into(),
            message: message.into(),
        }
    }
    
    /// Create a timeout error
    pub fn timeout<S: Into<String>>(operation: S, timeout_seconds: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout_seconds,
        }
    }
    
    /// Create a validation error
    pub fn validation<S: Into<String>>(field: S, message: S) -> Self {
        Self::Validation {
            field: field.into(),
            message: message.into(),
        }
    }
    
    /// Create a service unavailable error
    pub fn service_unavailable<S: Into<String>>(service: S) -> Self {
        Self::ServiceUnavailable {
            service: service.into(),
        }
    }
    
    /// Create an internal error
    pub fn internal<S: Into<String>>(message: S) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
    
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::Network { .. } => true,
            Self::Timeout { .. } => true,
            Self::Api { .. } => true,
            Self::CalendarSync { .. } => true,
            Self::Authentication { .. } => false, // Usually requires user intervention
            Self::Config { .. } => false,
            Self::Database { .. } => false,
            Self::FileSystem { .. } => false,
            Self::Parsing { .. } => false,
            Self::Validation { .. } => false,
            Self::ServiceUnavailable { .. } => false,
            Self::Internal { .. } => false,
        }
    }
    
    /// Get the error category for grouping
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Config { .. } => ErrorCategory::Configuration,
            Self::Database { .. } => ErrorCategory::Database,
            Self::CalendarSync { .. } => ErrorCategory::CalendarSync,
            Self::Authentication { .. } => ErrorCategory::Authentication,
            Self::Api { .. } => ErrorCategory::Api,
            Self::Network { .. } => ErrorCategory::Network,
            Self::FileSystem { .. } => ErrorCategory::FileSystem,
            Self::Parsing { .. } => ErrorCategory::Parsing,
            Self::Timeout { .. } => ErrorCategory::Timeout,
            Self::Validation { .. } => ErrorCategory::Validation,
            Self::ServiceUnavailable { .. } => ErrorCategory::ServiceUnavailable,
            Self::Internal { .. } => ErrorCategory::Internal,
        }
    }
}

/// Error categories for grouping and handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCategory {
    Configuration,
    Database,
    CalendarSync,
    Authentication,
    Api,
    Network,
    FileSystem,
    Parsing,
    Timeout,
    Validation,
    ServiceUnavailable,
    Internal,
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