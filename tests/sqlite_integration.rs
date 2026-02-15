use serde_json::Value;

mod common;
use common::*;

#[tokio::test]
async fn test_list_tables() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let tables = mcp_sql::db::dialect::list_tables(&pool, mcp_sql::db::DbBackend::Sqlite)
        .await
        .unwrap();

    let names: Vec<&str> = tables
        .iter()
        .filter_map(|t| t.get("table_name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"users"));
    assert!(names.contains(&"posts"));
}

#[tokio::test]
async fn test_describe_table() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let columns = mcp_sql::db::dialect::describe_table(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "users",
    )
    .await
    .unwrap();

    assert!(!columns.is_empty());

    let col_names: Vec<&str> = columns
        .iter()
        .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(col_names.contains(&"id"));
    assert!(col_names.contains(&"name"));
    assert!(col_names.contains(&"email"));

    // Check that `id` is marked as primary key
    let id_col = columns
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("id"))
        .unwrap();
    assert_eq!(id_col.get("primary_key").and_then(|v| v.as_str()), Some("YES"));
}

#[tokio::test]
async fn test_describe_nonexistent_table() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let result = mcp_sql::db::dialect::describe_table(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "nonexistent",
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_query_with_row_to_json() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let rows = sqlx::query("SELECT id, name, email, active FROM users ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();

    let results: Vec<Value> = rows.iter().map(mcp_sql::db::convert::row_to_json).collect();

    assert_eq!(results.len(), 2);

    let first = &results[0];
    assert_eq!(first.get("name").and_then(|v| v.as_str()), Some("Alice"));
    assert_eq!(
        first.get("email").and_then(|v| v.as_str()),
        Some("alice@example.com")
    );

    let second = &results[1];
    assert_eq!(second.get("name").and_then(|v| v.as_str()), Some("Bob"));
}

#[tokio::test]
async fn test_query_null_handling() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    // Insert a row with NULL email
    sqlx::query("INSERT INTO users (name, email, active) VALUES ('Charlie', NULL, 1)")
        .execute(&pool)
        .await
        .unwrap();

    let rows = sqlx::query("SELECT name, email FROM users WHERE name = 'Charlie'")
        .fetch_all(&pool)
        .await
        .unwrap();

    let result = mcp_sql::db::convert::row_to_json(&rows[0]);
    assert_eq!(result.get("email"), Some(&Value::Null));
}

#[tokio::test]
async fn test_database_manager_single_db() {
    sqlx::any::install_default_drivers();
    let db = mcp_sql::db::DatabaseManager::new(&["sqlite::memory:".to_string()])
        .await
        .unwrap();

    // Should resolve without specifying database name
    let entry = db.resolve(None).unwrap();
    assert_eq!(entry.name, "memory");
    assert_eq!(entry.backend, mcp_sql::db::DbBackend::Sqlite);

    // Should also resolve with explicit name
    let entry = db.resolve(Some("memory")).unwrap();
    assert_eq!(entry.name, "memory");
}

#[tokio::test]
async fn test_database_manager_multiple_dbs() {
    sqlx::any::install_default_drivers();
    let db = mcp_sql::db::DatabaseManager::new(&[
        "sqlite::memory:".to_string(),
        "sqlite::memory:".to_string(),
    ])
    .await
    .unwrap();

    // Should fail without specifying database name
    assert!(db.resolve(None).is_err());
}

#[tokio::test]
async fn test_database_manager_not_found() {
    sqlx::any::install_default_drivers();
    let db = mcp_sql::db::DatabaseManager::new(&["sqlite::memory:".to_string()])
        .await
        .unwrap();

    assert!(db.resolve(Some("nonexistent")).is_err());
}

#[tokio::test]
async fn test_explain() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let prefix = mcp_sql::db::dialect::explain_prefix(mcp_sql::db::DbBackend::Sqlite);
    let explain_sql = format!("{prefix}SELECT * FROM users WHERE id = 1");
    let rows = sqlx::query(&explain_sql).fetch_all(&pool).await.unwrap();

    // EXPLAIN QUERY PLAN should return at least one row
    assert!(!rows.is_empty());
}

#[tokio::test]
async fn test_sample_data() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let rows = mcp_sql::db::dialect::sample_data(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "users",
        5,
    )
    .await
    .unwrap();

    // We inserted 2 users, so we should get 2 back
    assert_eq!(rows.len(), 2);

    // Each row should have expected columns
    let first = &rows[0];
    assert!(first.get("id").is_some());
    assert!(first.get("name").is_some());
    assert!(first.get("email").is_some());
}

#[tokio::test]
async fn test_sample_data_with_limit() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let rows = mcp_sql::db::dialect::sample_data(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "users",
        1,
    )
    .await
    .unwrap();

    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_sample_data_invalid_table() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    let result = mcp_sql::db::dialect::sample_data(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "nonexistent",
        5,
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_describe_table_foreign_keys() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    // posts table has a FK from user_id -> users(id)
    let columns = mcp_sql::db::dialect::describe_table(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "posts",
    )
    .await
    .unwrap();

    // Find the user_id column
    let user_id_col = columns
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("user_id"))
        .expect("user_id column should exist");

    // Should have a foreign_key field pointing to users.id
    let fk = user_id_col.get("foreign_key").and_then(|v| v.as_str());
    assert_eq!(fk, Some("users.id"));

    // The id column should have no FK
    let id_col = columns
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some("id"))
        .expect("id column should exist");

    assert!(
        id_col.get("foreign_key").unwrap().is_null(),
        "id column should have null foreign_key"
    );
}

#[tokio::test]
async fn test_describe_table_no_foreign_keys() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;
    setup_test_schema(&pool).await;

    // users table has no FKs
    let columns = mcp_sql::db::dialect::describe_table(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "users",
    )
    .await
    .unwrap();

    // All columns should have null foreign_key
    for col in &columns {
        let fk = col.get("foreign_key");
        assert!(
            fk.is_none() || fk.unwrap().is_null(),
            "users table should have no foreign keys, but {:?} has {:?}",
            col.get("name"),
            fk
        );
    }
}

#[tokio::test]
async fn test_numeric_types() {
    sqlx::any::install_default_drivers();
    let pool = create_test_pool().await;

    sqlx::query(
        "CREATE TABLE numbers (
            int_val INTEGER,
            real_val REAL,
            text_val TEXT
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO numbers VALUES (42, 3.14, 'hello')")
        .execute(&pool)
        .await
        .unwrap();

    let rows = sqlx::query("SELECT * FROM numbers")
        .fetch_all(&pool)
        .await
        .unwrap();

    let result = mcp_sql::db::convert::row_to_json(&rows[0]);

    // Integer should come back as a number
    assert!(result.get("int_val").unwrap().is_number());
    assert_eq!(result.get("int_val").unwrap().as_i64(), Some(42));

    // Real should come back as a number
    assert!(result.get("real_val").unwrap().is_number());

    // Text should come back as a string
    assert_eq!(
        result.get("text_val").and_then(|v| v.as_str()),
        Some("hello")
    );
}
