use serde_json::Value;
use sqlx::{AnyPool, Row};

use crate::db::convert::row_to_json;
use crate::db::DbBackend;
use crate::error::McpSqlError;

/// List tables with approximate row counts.
pub async fn list_tables(pool: &AnyPool, backend: DbBackend) -> Result<Vec<Value>, McpSqlError> {
    let sql = match backend {
        DbBackend::Postgres => {
            "SELECT schemaname || '.' || tablename AS table_name, \
                    COALESCE(n_live_tup, 0) AS row_count \
             FROM pg_tables \
             LEFT JOIN pg_stat_user_tables ON tablename = relname AND schemaname = pg_stat_user_tables.schemaname \
             WHERE pg_tables.schemaname NOT IN ('pg_catalog', 'information_schema') \
             ORDER BY table_name"
        }
        DbBackend::Sqlite => {
            "SELECT name AS table_name, 0 AS row_count \
             FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name"
        }
        DbBackend::Mysql => {
            "SELECT table_name, table_rows AS row_count \
             FROM information_schema.tables \
             WHERE table_schema = DATABASE() \
             ORDER BY table_name"
        }
    };

    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(rows.iter().map(row_to_json).collect())
}

/// Describe a table's columns.
pub async fn describe_table(
    pool: &AnyPool,
    backend: DbBackend,
    table: &str,
) -> Result<Vec<Value>, McpSqlError> {
    match backend {
        DbBackend::Postgres => describe_table_postgres(pool, table).await,
        DbBackend::Sqlite => describe_table_sqlite(pool, table).await,
        DbBackend::Mysql => describe_table_mysql(pool, table).await,
    }
}

async fn describe_table_postgres(pool: &AnyPool, table: &str) -> Result<Vec<Value>, McpSqlError> {
    // Handle schema.table format
    let (schema, tbl) = if let Some((s, t)) = table.split_once('.') {
        (s, t)
    } else {
        ("public", table)
    };

    let sql = "SELECT c.column_name AS name, c.data_type AS type, \
               c.is_nullable AS nullable, c.column_default AS default_value, \
               CASE WHEN tc.constraint_type = 'PRIMARY KEY' THEN 'YES' ELSE 'NO' END AS primary_key \
               FROM information_schema.columns c \
               LEFT JOIN information_schema.key_column_usage kcu \
                 ON c.table_schema = kcu.table_schema \
                 AND c.table_name = kcu.table_name \
                 AND c.column_name = kcu.column_name \
               LEFT JOIN information_schema.table_constraints tc \
                 ON kcu.constraint_name = tc.constraint_name \
                 AND kcu.table_schema = tc.table_schema \
                 AND tc.constraint_type = 'PRIMARY KEY' \
               WHERE c.table_schema = $1 AND c.table_name = $2 \
               ORDER BY c.ordinal_position";

    let rows = sqlx::query(sql)
        .bind(schema)
        .bind(tbl)
        .fetch_all(pool)
        .await?;

    if rows.is_empty() {
        return Err(McpSqlError::Other(format!("Table '{table}' not found")));
    }

    Ok(rows.iter().map(row_to_json).collect())
}

async fn describe_table_sqlite(pool: &AnyPool, table: &str) -> Result<Vec<Value>, McpSqlError> {
    // SQLite PRAGMA doesn't support parameterized queries, so we validate the table name
    let safe_table = sanitize_identifier(table)?;
    let sql = format!("PRAGMA table_info(\"{}\")", safe_table);
    let rows = sqlx::query(&sql).fetch_all(pool).await?;

    if rows.is_empty() {
        return Err(McpSqlError::Other(format!("Table '{table}' not found")));
    }

    let mut result = Vec::new();
    for row in &rows {
        let name: String = row.try_get("name").unwrap_or_default();
        let col_type: String = row.try_get("type").unwrap_or_default();
        let notnull: i32 = row.try_get("notnull").unwrap_or(0);
        let dflt_value: Option<String> = row.try_get("dflt_value").ok();
        let pk: i32 = row.try_get("pk").unwrap_or(0);

        result.push(serde_json::json!({
            "name": name,
            "type": col_type,
            "nullable": if notnull == 0 { "YES" } else { "NO" },
            "default_value": dflt_value,
            "primary_key": if pk > 0 { "YES" } else { "NO" },
        }));
    }

    Ok(result)
}

async fn describe_table_mysql(pool: &AnyPool, table: &str) -> Result<Vec<Value>, McpSqlError> {
    let sql = "SELECT column_name AS name, column_type AS type, \
               is_nullable AS nullable, column_default AS default_value, \
               CASE WHEN column_key = 'PRI' THEN 'YES' ELSE 'NO' END AS primary_key \
               FROM information_schema.columns \
               WHERE table_schema = DATABASE() AND table_name = ? \
               ORDER BY ordinal_position";

    let rows = sqlx::query(sql).bind(table).fetch_all(pool).await?;

    if rows.is_empty() {
        return Err(McpSqlError::Other(format!("Table '{table}' not found")));
    }

    Ok(rows.iter().map(row_to_json).collect())
}

/// Get the correct EXPLAIN prefix for each backend.
pub fn explain_prefix(backend: DbBackend) -> &'static str {
    match backend {
        DbBackend::Postgres => "EXPLAIN (FORMAT TEXT) ",
        DbBackend::Sqlite => "EXPLAIN QUERY PLAN ",
        DbBackend::Mysql => "EXPLAIN ",
    }
}

/// Validate and sanitize a SQL identifier to prevent injection.
fn sanitize_identifier(name: &str) -> Result<String, McpSqlError> {
    // Allow alphanumeric, underscore, dot (for schema.table), and hyphen
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
        && !name.is_empty()
    {
        Ok(name.to_string())
    } else {
        Err(McpSqlError::InvalidSql(format!(
            "Invalid identifier: '{name}'"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_identifier() {
        assert!(sanitize_identifier("users").is_ok());
        assert!(sanitize_identifier("public.users").is_ok());
        assert!(sanitize_identifier("my_table").is_ok());
        assert!(sanitize_identifier("my-table").is_ok());
        assert!(sanitize_identifier("").is_err());
        assert!(sanitize_identifier("users; DROP TABLE users").is_err());
        assert!(sanitize_identifier("users\"").is_err());
    }

    #[test]
    fn test_explain_prefix() {
        assert_eq!(explain_prefix(DbBackend::Postgres), "EXPLAIN (FORMAT TEXT) ");
        assert_eq!(explain_prefix(DbBackend::Sqlite), "EXPLAIN QUERY PLAN ");
        assert_eq!(explain_prefix(DbBackend::Mysql), "EXPLAIN ");
    }
}
