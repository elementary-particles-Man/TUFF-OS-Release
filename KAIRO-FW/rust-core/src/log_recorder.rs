//! Minimal stub to satisfy workspace builds.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub category: String,
    pub message: String,
}

pub fn record(category: impl Into<String>, message: impl Into<String>) -> LogRecord {
    LogRecord {
        category: category.into(),
        message: message.into(),
    }
}
