use std::sync::Arc;
use std::time::Duration;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;

use crate::db::convert::row_to_json;
use crate::db::dialect;
use crate::db::{DatabaseManager, DbBackend};
use crate::error::McpSqlError;

#[derive(Clone)]
pub struct McpSqlServer {
    db: Arc<DatabaseManager>,
    allow_write: bool,
    row_limit: u32,
    query_timeout: Duration,
    tool_router: ToolRouter<Self>,
}

// -- Tool parameter types --

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DatabaseParam {
    #[schemars(description = "Database name (optional if only one database is connected)")]
    #[serde(default)]
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DescribeTableParams {
    #[schemars(description = "Table name to describe (use schema.table for PostgreSQL)")]
    pub table: String,

    #[schemars(description = "Database name (optional if only one database is connected)")]
    #[serde(default)]
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SampleDataParams {
    #[schemars(description = "Table name to sample rows from")]
    pub table: String,

    #[schemars(description = "Database name (optional if only one database is connected)")]
    #[serde(default)]
    pub database: Option<String>,

    #[schemars(description = "Number of sample rows to return (default: 5)")]
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryParams {
    #[schemars(description = "SQL query to execute")]
    pub sql: String,

    #[schemars(description = "Database name (optional if only one database is connected)")]
    #[serde(default)]
    pub database: Option<String>,
}

impl McpSqlServer {
    pub fn new(db: DatabaseManager, allow_write: bool, row_limit: u32, query_timeout_secs: u64) -> Self {
        Self {
            db: Arc::new(db),
            allow_write,
            row_limit,
            query_timeout: Duration::from_secs(query_timeout_secs),
            tool_router: Self::tool_router(),
        }
    }

    fn err(&self, e: McpSqlError) -> ErrorData {
        e.to_mcp_error()
    }
}

#[tool_router]
impl McpSqlServer {
    #[tool(
        name = "list_databases",
        description = "List all connected databases with their names and types (postgres/sqlite/mysql)"
    )]
    async fn list_databases(&self) -> Result<CallToolResult, ErrorData> {
        let databases: Vec<serde_json::Value> = self
            .db
            .databases
            .iter()
            .map(|d| {
                serde_json::json!({
                    "name": d.name,
                    "type": d.backend.name(),
                    "url": d.url_redacted,
                })
            })
            .collect();

        let text = serde_json::to_string_pretty(&databases)
            .unwrap_or_else(|_| "[]".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "list_tables",
        description = "List all tables in a database with approximate row counts"
    )]
    async fn list_tables(
        &self,
        Parameters(params): Parameters<DatabaseParam>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let tables = dialect::list_tables(&entry.pool, entry.backend)
            .await
            .map_err(|e| self.err(e))?;

        let text = serde_json::to_string_pretty(&tables)
            .unwrap_or_else(|_| "[]".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "describe_table",
        description = "Describe a table's columns with name, type, nullable, default, and primary key info"
    )]
    async fn describe_table(
        &self,
        Parameters(params): Parameters<DescribeTableParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let columns = dialect::describe_table(&entry.pool, entry.backend, &params.table)
            .await
            .map_err(|e| self.err(e))?;

        let text = serde_json::to_string_pretty(&columns)
            .unwrap_or_else(|_| "[]".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "query",
        description = "Execute a SQL query and return results as JSON. Read-only by default (SELECT/WITH/SHOW/PRAGMA only). Use --allow-write flag to enable write operations."
    )]
    async fn query(
        &self,
        Parameters(params): Parameters<QueryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let sql = params.sql.trim();

        // Read-only guard
        if !self.allow_write {
            check_read_only(sql).map_err(|e| self.err(e))?;
        }

        // Set transaction read only for backends that support it
        if !self.allow_write && entry.backend != DbBackend::Sqlite {
            let read_only_sql = match entry.backend {
                DbBackend::Postgres => "SET TRANSACTION READ ONLY",
                DbBackend::Mysql => "SET TRANSACTION READ ONLY",
                DbBackend::Sqlite => unreachable!(),
            };
            // Best effort — some connection states may not support this
            let _ = sqlx::query(read_only_sql).execute(&entry.pool).await;
        }

        // Inject LIMIT if not present
        let limited_sql = inject_limit(sql, self.row_limit);

        let rows = tokio::time::timeout(
            self.query_timeout,
            sqlx::query(&limited_sql).fetch_all(&entry.pool),
        )
        .await
        .map_err(|_| self.err(McpSqlError::QueryTimeout(self.query_timeout.as_secs())))?
        .map_err(|e| self.err(McpSqlError::Database(e)))?;

        let results: Vec<serde_json::Value> = rows.iter().map(row_to_json).collect();
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "rows": results,
            "count": results.len(),
        }))
        .unwrap_or_else(|_| "{}".to_string());

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "explain",
        description = "Show the query execution plan for a SQL statement. Uses the appropriate EXPLAIN syntax for the database type."
    )]
    async fn explain(
        &self,
        Parameters(params): Parameters<QueryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let prefix = dialect::explain_prefix(entry.backend);
        let explain_sql = format!("{}{}", prefix, params.sql.trim());

        let rows = tokio::time::timeout(
            self.query_timeout,
            sqlx::query(&explain_sql).fetch_all(&entry.pool),
        )
        .await
        .map_err(|_| self.err(McpSqlError::QueryTimeout(self.query_timeout.as_secs())))?
        .map_err(|e| self.err(McpSqlError::Database(e)))?;

        let results: Vec<serde_json::Value> = rows.iter().map(row_to_json).collect();
        let text = serde_json::to_string_pretty(&results)
            .unwrap_or_else(|_| "[]".to_string());

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "sample_data",
        description = "Return sample rows from a table as JSON. Useful for previewing table contents without writing SQL."
    )]
    async fn sample_data(
        &self,
        Parameters(params): Parameters<SampleDataParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let limit = params.limit.unwrap_or(5);

        let rows = tokio::time::timeout(
            self.query_timeout,
            dialect::sample_data(&entry.pool, entry.backend, &params.table, limit),
        )
        .await
        .map_err(|_| self.err(McpSqlError::QueryTimeout(self.query_timeout.as_secs())))?
        .map_err(|e| self.err(e))?;

        let text = serde_json::to_string_pretty(&serde_json::json!({
            "table": params.table,
            "rows": rows,
            "count": rows.len(),
        }))
        .unwrap_or_else(|_| "{}".to_string());

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[tool_handler]
impl ServerHandler for McpSqlServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "mcp-sql".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: Some(
                "SQL database server. Use list_databases to see connected databases, \
                 list_tables to see tables, describe_table for schema details (includes foreign keys), \
                 sample_data to preview table contents, query to run SQL, and explain for query plans."
                    .to_string(),
            ),
        }
    }
}

/// Heuristic check: only allow SELECT, WITH, SHOW, PRAGMA, EXPLAIN.
fn check_read_only(sql: &str) -> Result<(), McpSqlError> {
    let upper = sql.trim_start().to_uppercase();
    let allowed_prefixes = ["SELECT", "WITH", "SHOW", "PRAGMA", "EXPLAIN"];
    if allowed_prefixes.iter().any(|p| upper.starts_with(p)) {
        Ok(())
    } else {
        Err(McpSqlError::ReadOnly(
            "Only SELECT/WITH/SHOW/PRAGMA/EXPLAIN queries are allowed in read-only mode. \
             Start the server with --allow-write to enable write operations."
                .to_string(),
        ))
    }
}

/// Inject a LIMIT clause if the query doesn't already have one.
fn inject_limit(sql: &str, limit: u32) -> String {
    let upper = sql.to_uppercase();
    // Don't inject LIMIT for non-SELECT statements or if LIMIT already present
    if !upper.trim_start().starts_with("SELECT") && !upper.trim_start().starts_with("WITH") {
        return sql.to_string();
    }
    if upper.contains(" LIMIT ") {
        return sql.to_string();
    }
    // Strip trailing semicolon if present
    let trimmed = sql.trim_end().trim_end_matches(';');
    format!("{trimmed} LIMIT {limit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_read_only() {
        assert!(check_read_only("SELECT * FROM users").is_ok());
        assert!(check_read_only("  select * from users").is_ok());
        assert!(check_read_only("WITH cte AS (SELECT 1) SELECT * FROM cte").is_ok());
        assert!(check_read_only("SHOW TABLES").is_ok());
        assert!(check_read_only("PRAGMA table_info(users)").is_ok());
        assert!(check_read_only("EXPLAIN SELECT * FROM users").is_ok());

        assert!(check_read_only("INSERT INTO users VALUES (1)").is_err());
        assert!(check_read_only("UPDATE users SET name = 'x'").is_err());
        assert!(check_read_only("DELETE FROM users").is_err());
        assert!(check_read_only("DROP TABLE users").is_err());
        assert!(check_read_only("CREATE TABLE t (id INT)").is_err());
    }

    #[test]
    fn test_inject_limit() {
        assert_eq!(
            inject_limit("SELECT * FROM users", 100),
            "SELECT * FROM users LIMIT 100"
        );
        assert_eq!(
            inject_limit("SELECT * FROM users;", 100),
            "SELECT * FROM users LIMIT 100"
        );
        assert_eq!(
            inject_limit("SELECT * FROM users LIMIT 10", 100),
            "SELECT * FROM users LIMIT 10"
        );
        assert_eq!(
            inject_limit("INSERT INTO users VALUES (1)", 100),
            "INSERT INTO users VALUES (1)"
        );
        assert_eq!(
            inject_limit("WITH cte AS (SELECT 1) SELECT * FROM cte", 50),
            "WITH cte AS (SELECT 1) SELECT * FROM cte LIMIT 50"
        );
    }
}
