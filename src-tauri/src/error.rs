//! Serializable failures for the UI. Storage details stay in local diagnostics;
//! callers receive an actionable message instead of a raw database exception.

use serde::Serialize;

#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, AppError>;
impl AppError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "validation".into(),
            message: message.into(),
        }
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: "conflict".into(),
            message: message.into(),
        }
    }
    pub fn storage(message: impl Into<String>) -> Self {
        Self {
            code: "storage".into(),
            message: message.into(),
        }
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        log::error!("Database operation failed: {e}");
        Self::storage(
            "The collection could not be updated. Check available disk space and run an integrity check.",
        )
    }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        log::error!("Filesystem operation failed: {e}");
        Self::storage(
            "The file could not be read or written. Check its location, permissions, and available disk space.",
        )
    }
}
impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        Self::invalid("The data is not in a supported Tala format.")
    }
}
impl From<zip::result::ZipError> for AppError {
    fn from(_: zip::result::ZipError) -> Self {
        Self::invalid("This archive is damaged or is not a supported Tala backup.")
    }
}
impl From<csv::Error> for AppError {
    fn from(e: csv::Error) -> Self {
        Self::invalid(format!("The delimited file could not be read: {e}"))
    }
}
