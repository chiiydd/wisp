//! Core error types used across the entire workspace.

use thiserror::Error;

/// Top-level error type for `wisp-core` operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Path is blacklisted: {path}")]
    BlacklistedPath { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Path traversal attack detected: {path}")]
    PathTraversal { path: String },

    #[error("Unsupported platform: {platform}")]
    UnsupportedPlatform { platform: String },

    #[error("Cleaner not found: {id}")]
    CleanerNotFound { id: String },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Operation cancelled by user")]
    Cancelled,

    #[error("Insufficient privileges – root required")]
    InsufficientPrivileges,
}

/// Alias for `Result<T, CoreError>`.
pub type CoreResult<T> = std::result::Result<T, CoreError>;
