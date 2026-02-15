use rmcp::model::ErrorData;

#[derive(Debug, thiserror::Error)]
pub enum McpSqlError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Write operation rejected: {0}")]
    ReadOnly(String),

    #[error("Database not found: {0}")]
    DatabaseNotFound(String),

    #[error("Ambiguous database: multiple databases connected, specify the 'database' parameter")]
    AmbiguousDatabase,

    #[error("Invalid SQL: {0}")]
    InvalidSql(String),

    #[error("Query timed out after {0} seconds")]
    QueryTimeout(u64),

    #[error("{0}")]
    Other(String),
}

impl McpSqlError {
    pub fn to_mcp_error(&self) -> ErrorData {
        match self {
            McpSqlError::ReadOnly(_) | McpSqlError::InvalidSql(_) => {
                ErrorData::invalid_params(self.to_string(), None)
            }
            McpSqlError::DatabaseNotFound(_) | McpSqlError::AmbiguousDatabase => {
                ErrorData::invalid_params(self.to_string(), None)
            }
            McpSqlError::QueryTimeout(_) => {
                ErrorData::internal_error(self.to_string(), None)
            }
            McpSqlError::Database(_) | McpSqlError::Other(_) => {
                ErrorData::internal_error(self.to_string(), None)
            }
        }
    }
}
