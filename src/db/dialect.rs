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

    // Fetch FK info
    let fk_sql = "SELECT kcu.column_name, ccu.table_schema || '.' || ccu.table_name || '.' || ccu.column_name AS references_col \
                   FROM information_schema.key_column_usage kcu \
                   JOIN information_schema.referential_constraints rc \
                     ON kcu.constraint_name = rc.constraint_name AND kcu.constraint_schema = rc.constraint_schema \
                   JOIN information_schema.constraint_column_usage ccu \
                     ON rc.unique_constraint_name = ccu.constraint_name AND rc.unique_constraint_schema = ccu.constraint_schema \
                   WHERE kcu.table_schema = $1 AND kcu.table_name = $2";

    let fk_rows = sqlx::query(fk_sql)
        .bind(schema)
        .bind(tbl)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let fk_map: std::collections::HashMap<String, String> = fk_rows
        .iter()
        .filter_map(|r| {
            let col: String = r.try_get("column_name").ok()?;
            let refs: String = r.try_get("references_col").ok()?;
            Some((col, refs))
        })
        .collect();

    let mut result: Vec<Value> = rows.iter().map(row_to_json).collect();
    for col in &mut result {
        if let Value::Object(map) = col {
            let col_name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let fk = fk_map.get(col_name).map(|s| Value::String(s.clone())).unwrap_or(Value::Null);
            map.insert("foreign_key".to_string(), fk);
        }
    }

    Ok(result)
}

async fn describe_table_sqlite(pool: &AnyPool, table: &str) -> Result<Vec<Value>, McpSqlError> {
    // SQLite PRAGMA doesn't support parameterized queries, so we validate the table name
    let safe_table = sanitize_identifier(table)?;
    let sql = format!("PRAGMA table_info(\"{}\")", safe_table);
    let rows = sqlx::query(&sql).fetch_all(pool).await?;

    if rows.is_empty() {
        return Err(McpSqlError::Other(format!("Table '{table}' not found")));
    }

    // Fetch FK info via PRAGMA foreign_key_list
    let fk_sql = format!("PRAGMA foreign_key_list(\"{}\")", safe_table);
    let fk_rows = sqlx::query(&fk_sql).fetch_all(pool).await.unwrap_or_default();

    let fk_map: std::collections::HashMap<String, String> = fk_rows
        .iter()
        .filter_map(|r| {
            let from: String = r.try_get("from").ok()?;
            let ref_table: String = r.try_get("table").ok()?;
            let ref_col: String = r.try_get("to").ok()?;
            Some((from, format!("{ref_table}.{ref_col}")))
        })
        .collect();

    let mut result = Vec::new();
    for row in &rows {
        let name: String = row.try_get("name").unwrap_or_default();
        let col_type: String = row.try_get("type").unwrap_or_default();
        let notnull: i32 = row.try_get("notnull").unwrap_or(0);
        let dflt_value: Option<String> = row.try_get("dflt_value").ok();
        let pk: i32 = row.try_get("pk").unwrap_or(0);
        let fk = fk_map.get(&name).cloned();

        result.push(serde_json::json!({
            "name": name,
            "type": col_type,
            "nullable": if notnull == 0 { "YES" } else { "NO" },
            "default_value": dflt_value,
            "primary_key": if pk > 0 { "YES" } else { "NO" },
            "foreign_key": fk,
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

    // Fetch FK info
    let fk_sql = "SELECT column_name, CONCAT(referenced_table_name, '.', referenced_column_name) AS references_col \
                   FROM information_schema.key_column_usage \
                   WHERE table_schema = DATABASE() AND table_name = ? AND referenced_table_name IS NOT NULL";

    let fk_rows = sqlx::query(fk_sql).bind(table).fetch_all(pool).await.unwrap_or_default();

    let fk_map: std::collections::HashMap<String, String> = fk_rows
        .iter()
        .filter_map(|r| {
            let col: String = r.try_get("column_name").ok()?;
            let refs: String = r.try_get("references_col").ok()?;
            Some((col, refs))
        })
        .collect();

    let mut result: Vec<Value> = rows.iter().map(row_to_json).collect();
    for col in &mut result {
        if let Value::Object(map) = col {
            let col_name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let fk = fk_map.get(col_name).map(|s| Value::String(s.clone())).unwrap_or(Value::Null);
            map.insert("foreign_key".to_string(), fk);
        }
    }

    Ok(result)
}

/// Sample N rows from a table.
pub async fn sample_data(
    pool: &AnyPool,
    backend: DbBackend,
    table: &str,
    limit: u32,
) -> Result<Vec<Value>, McpSqlError> {
    let safe_table = sanitize_identifier(table)?;
    let sql = match backend {
        DbBackend::Postgres => format!(
            "SELECT * FROM \"{}\" TABLESAMPLE BERNOULLI (100) LIMIT {}",
            safe_table, limit
        ),
        DbBackend::Sqlite => format!("SELECT * FROM \"{}\" LIMIT {}", safe_table, limit),
        DbBackend::Mysql => format!(
            "SELECT * FROM `{}` ORDER BY RAND() LIMIT {}",
            safe_table, limit
        ),
    };

    let rows = sqlx::query(&sql).fetch_all(pool).await?;
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
