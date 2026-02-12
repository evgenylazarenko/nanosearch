use std::fmt;

/// Structured error type for nanosearch operations.
///
/// Replaces `Box<dyn Error>` across the public API so callers can
/// distinguish error kinds (e.g. missing index vs. corrupt meta vs.
/// query parse failure) and produce targeted, actionable messages.
#[derive(Debug)]
pub enum NsError {
    /// File system I/O failure.
    Io(std::io::Error),
    /// Tantivy index operation failure (open, create, search, commit).
    Tantivy(tantivy::TantivyError),
    /// Tantivy query parse failure (invalid query syntax).
    QueryParse(tantivy::query::QueryParserError),
    /// meta.json serialization/deserialization failure.
    Json(serde_json::Error),
    /// Index schema version does not match the current binary.
    SchemaVersionMismatch { found: u32, expected: u32 },
}

impl fmt::Display for NsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NsError::Io(e) => write!(f, "{}", e),
            NsError::Tantivy(e) => write!(f, "{}", e),
            NsError::QueryParse(e) => write!(f, "query parse error: {}", e),
            NsError::Json(e) => write!(f, "meta.json error: {}", e),
            NsError::SchemaVersionMismatch { found, expected } => write!(
                f,
                "index schema version {} does not match expected version {} — run `ns index` to rebuild",
                found, expected
            ),
        }
    }
}

impl std::error::Error for NsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NsError::Io(e) => Some(e),
            NsError::Tantivy(e) => Some(e),
            NsError::QueryParse(e) => Some(e),
            NsError::Json(e) => Some(e),
            NsError::SchemaVersionMismatch { .. } => None,
        }
    }
}

impl From<std::io::Error> for NsError {
    fn from(e: std::io::Error) -> Self {
        NsError::Io(e)
    }
}

impl From<tantivy::TantivyError> for NsError {
    fn from(e: tantivy::TantivyError) -> Self {
        NsError::Tantivy(e)
    }
}

impl From<tantivy::query::QueryParserError> for NsError {
    fn from(e: tantivy::query::QueryParserError) -> Self {
        NsError::QueryParse(e)
    }
}

impl From<serde_json::Error> for NsError {
    fn from(e: serde_json::Error) -> Self {
        NsError::Json(e)
    }
}
