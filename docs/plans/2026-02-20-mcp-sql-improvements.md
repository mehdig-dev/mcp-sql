# mcp-sql Improvements Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add 3 new MCP tools (show_schema, show_create_table, query_dry_run), improve existing tools, add --demo flag, and prepare for distribution — bringing mcp-sql from v0.1.0 to v0.2.0.

**Architecture:** All new tools follow the existing pattern: `#[tool]` handler in `server.rs`, backend-specific SQL in `dialect.rs`, row conversion via `convert.rs`. The `--demo` flag creates an in-memory SQLite database with sample data before starting the MCP server.

**Tech Stack:** Rust, rmcp 0.15, sqlx 0.8 (any driver), clap 4, tokio, schemars 1

---

### Task 1: Add `--demo` flag with sample database

**Files:**
- Modify: `src/main.rs`
- Create: `src/demo.rs`
- Modify: `src/lib.rs`
- Test: `tests/sqlite_integration.rs`

**Step 1: Create `src/demo.rs` with sample schema**

```rust
use sqlx::AnyPool;

/// Creates an in-memory SQLite database with sample tables for demo mode.
pub async fn create_demo_database() -> Result<AnyPool, sqlx::Error> {
    let pool = AnyPool::connect("sqlite::memory:").await?;

    sqlx::raw_sql(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'user',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            body TEXT,
            status TEXT NOT NULL DEFAULT 'draft',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        CREATE TABLE comments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            post_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (post_id) REFERENCES posts(id),
            FOREIGN KEY (user_id) REFERENCES users(id)
        );

        INSERT INTO users (name, email, role) VALUES
            ('Alice Chen', 'alice@example.com', 'admin'),
            ('Bob Smith', 'bob@example.com', 'user'),
            ('Carol Davis', 'carol@example.com', 'user'),
            ('Dan Wilson', 'dan@example.com', 'editor'),
            ('Eve Taylor', 'eve@example.com', 'user'),
            ('Frank Brown', 'frank@example.com', 'user'),
            ('Grace Lee', 'grace@example.com', 'editor'),
            ('Hank Moore', 'hank@example.com', 'user'),
            ('Iris Clark', 'iris@example.com', 'user'),
            ('Jack White', 'jack@example.com', 'admin');

        INSERT INTO posts (user_id, title, body, status) VALUES
            (1, 'Getting Started with SQL', 'A beginner guide to SQL queries.', 'published'),
            (1, 'Advanced Joins Explained', 'Understanding INNER, LEFT, RIGHT, and FULL joins.', 'published'),
            (2, 'My First Post', 'Hello world from Bob!', 'published'),
            (3, 'Database Indexing Tips', 'How to speed up your queries with indexes.', 'published'),
            (4, 'Draft: New Feature Announcement', 'Coming soon...', 'draft'),
            (1, 'Understanding Transactions', 'ACID properties and isolation levels.', 'published'),
            (5, 'SQL vs NoSQL', 'When to choose each approach.', 'published'),
            (7, 'Data Modeling Best Practices', 'Normalization and denormalization trade-offs.', 'published'),
            (2, 'Query Optimization Guide', 'Tips for writing efficient SQL.', 'draft'),
            (3, 'Working with NULL Values', 'Common pitfalls with NULL in SQL.', 'published');

        INSERT INTO comments (post_id, user_id, body) VALUES
            (1, 2, 'Great introduction! Very helpful.'),
            (1, 3, 'Could you add examples with subqueries?'),
            (1, 5, 'Bookmarked for reference.'),
            (2, 3, 'The join diagrams really helped me understand.'),
            (2, 6, 'What about CROSS JOIN?'),
            (3, 1, 'Welcome to the community, Bob!'),
            (4, 1, 'Excellent tips on covering indexes.'),
            (4, 2, 'This saved me hours of debugging slow queries.'),
            (6, 4, 'Great explanation of isolation levels.'),
            (7, 8, 'We switched from SQL to NoSQL and regretted it.'),
            (7, 9, 'It really depends on the use case.'),
            (8, 1, 'Denormalization is underrated for read-heavy workloads.'),
            (10, 2, 'I always forget about three-valued logic with NULL.'),
            (10, 6, 'COALESCE is my best friend now.'),
            (10, 7, 'The IS NULL vs = NULL distinction trips everyone up.');",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}
```

**Step 2: Add `demo` module to `src/lib.rs`**

Add `pub mod demo;` to `src/lib.rs`:

```rust
pub mod db;
pub mod demo;
pub mod error;
pub mod server;
```

**Step 3: Add `--demo` flag to CLI and wire it up in `src/main.rs`**

Add to the `Cli` struct after the `query_timeout` field:

```rust
    /// Start with a demo SQLite database pre-loaded with sample data
    #[arg(long)]
    demo: bool,
```

In the `main()` function, after URL collection and before the "no URLs" error check, add the demo path:

```rust
    if cli.demo {
        let pool = mcp_sql::demo::create_demo_database()
            .await
            .expect("failed to create demo database");
        let entry = mcp_sql::db::DatabaseEntry {
            name: "demo".to_string(),
            pool,
            backend: mcp_sql::db::DbBackend::Sqlite,
            url_redacted: "sqlite::memory: (demo)".to_string(),
        };
        let manager = mcp_sql::db::DatabaseManager {
            databases: vec![entry],
        };
        let server = McpSqlServer::new(manager, false, cli.row_limit, cli.query_timeout);
        tracing::info!("mcp-sql demo mode — SQLite with sample tables (users, posts, comments)");
        let transport = rmcp::transport::io::stdio();
        let ct = server.serve(transport).await.expect("server error");
        ct.waiting().await;
        return;
    }
```

This requires `DatabaseManager.databases` to be `pub`. Check the struct definition — if `databases` is already `pub` then no change needed. If not, make it `pub`.

**Step 4: Write test for demo database**

Add to `tests/sqlite_integration.rs`:

```rust
#[tokio::test]
async fn test_demo_database() {
    sqlx::any::install_default_drivers();
    let pool = mcp_sql::demo::create_demo_database().await.unwrap();

    // Verify tables exist
    let tables: Vec<sqlx::any::AnyRow> = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .fetch_all(&pool)
        .await
        .unwrap();
    let names: Vec<String> = tables.iter().map(|r| r.get::<String, _>("name")).collect();
    assert!(names.contains(&"users".to_string()));
    assert!(names.contains(&"posts".to_string()));
    assert!(names.contains(&"comments".to_string()));

    // Verify row counts
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 10);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM posts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 10);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM comments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 15);

    // Verify FK relationships
    let fk_rows: Vec<sqlx::any::AnyRow> = sqlx::query("SELECT u.name, p.title FROM posts p JOIN users u ON p.user_id = u.id WHERE u.name = 'Alice Chen'")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(fk_rows.len(), 3); // Alice has 3 posts
}
```

**Step 5: Run tests**

```bash
cd /home/mehdi/projects/mcp-sql && cargo test
```

Expected: All existing tests + `test_demo_database` pass.

**Step 6: Commit**

```bash
git add src/demo.rs src/lib.rs src/main.rs tests/sqlite_integration.rs
git commit -m "feat: add --demo flag with sample SQLite database"
```

---

### Task 2: Add `show_create_table` tool

**Files:**
- Modify: `src/db/dialect.rs` (add `show_create_table()` function)
- Modify: `src/server.rs` (add tool handler)
- Test: `tests/sqlite_integration.rs`

**Step 1: Add `show_create_table()` to `src/db/dialect.rs`**

Add after the `describe_table()` function (around line 48):

```rust
/// Returns the CREATE TABLE DDL for a given table.
pub async fn show_create_table(
    pool: &AnyPool,
    backend: DbBackend,
    table: &str,
) -> Result<String, McpSqlError> {
    let table = sanitize_identifier(table)?;

    match backend {
        DbBackend::Sqlite => {
            let sql = format!("SELECT sql FROM sqlite_master WHERE type='table' AND name='{table}'");
            let row = sqlx::query(&sql)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| McpSqlError::Other(format!("table '{table}' not found")))?;
            let ddl: String = row.try_get("sql")?;
            Ok(ddl)
        }
        DbBackend::Mysql => {
            let sql = format!("SHOW CREATE TABLE `{table}`");
            let row = sqlx::query(&sql)
                .fetch_one(pool)
                .await
                .map_err(|_| McpSqlError::Other(format!("table '{table}' not found")))?;
            // MySQL returns two columns: "Table" and "Create Table"
            let ddl: String = row.try_get(1)?;
            Ok(ddl)
        }
        DbBackend::Postgres => {
            // PostgreSQL has no built-in SHOW CREATE TABLE.
            // Reconstruct from information_schema.
            let rows = sqlx::query(
                "SELECT column_name, data_type, is_nullable, column_default \
                 FROM information_schema.columns \
                 WHERE table_name = $1 \
                 ORDER BY ordinal_position",
            )
            .bind(&table)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Err(McpSqlError::Other(format!("table '{table}' not found")));
            }

            let mut ddl = format!("CREATE TABLE {table} (\n");
            for (i, row) in rows.iter().enumerate() {
                let name: String = row.try_get("column_name")?;
                let dtype: String = row.try_get("data_type")?;
                let nullable: String = row.try_get("is_nullable")?;
                let default: Option<String> = row.try_get("column_default").ok();

                ddl.push_str(&format!("    {name} {}", dtype.to_uppercase()));
                if nullable == "NO" {
                    ddl.push_str(" NOT NULL");
                }
                if let Some(def) = default {
                    ddl.push_str(&format!(" DEFAULT {def}"));
                }
                if i < rows.len() - 1 {
                    ddl.push(',');
                }
                ddl.push('\n');
            }
            ddl.push_str(");");
            Ok(ddl)
        }
    }
}
```

**Step 2: Add tool handler to `src/server.rs`**

Add the parameter type (near other param types around line 26):

```rust
#[derive(Debug, Deserialize, JsonSchema)]
struct ShowCreateTableParams {
    /// Table name
    table: String,
    /// Database name (optional if only one database is connected)
    database: Option<String>,
}
```

Add the tool handler inside the `#[tool_router]` impl block (after `sample_data` handler):

```rust
    #[tool(
        name = "show_create_table",
        description = "Show the CREATE TABLE DDL statement for a table"
    )]
    async fn show_create_table(
        &self,
        Parameters(params): Parameters<ShowCreateTableParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let ddl = dialect::show_create_table(&entry.pool, entry.backend, &params.table)
            .await
            .map_err(|e| self.err(e))?;
        Ok(CallToolResult::success(vec![Content::text(ddl)]))
    }
```

**Step 3: Write test**

Add to `tests/sqlite_integration.rs`:

```rust
#[tokio::test]
async fn test_show_create_table() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    common::setup_test_schema(&pool).await;

    let ddl = mcp_sql::db::dialect::show_create_table(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "users",
    )
    .await
    .unwrap();

    assert!(ddl.contains("CREATE TABLE"), "DDL should contain CREATE TABLE");
    assert!(ddl.contains("users"), "DDL should reference the table name");
    assert!(ddl.contains("name TEXT"), "DDL should include column definitions");
}

#[tokio::test]
async fn test_show_create_table_not_found() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    common::setup_test_schema(&pool).await;

    let result = mcp_sql::db::dialect::show_create_table(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "nonexistent",
    )
    .await;

    assert!(result.is_err());
}
```

**Step 4: Run tests**

```bash
cd /home/mehdi/projects/mcp-sql && cargo test
```

**Step 5: Commit**

```bash
git add src/server.rs src/db/dialect.rs tests/sqlite_integration.rs
git commit -m "feat: add show_create_table tool"
```

---

### Task 3: Add `show_schema` tool (Mermaid ER diagram)

**Files:**
- Create: `src/schema.rs`
- Modify: `src/lib.rs`
- Modify: `src/server.rs`

**Step 1: Create `src/schema.rs`**

```rust
use crate::db::{DbBackend, dialect};
use crate::error::McpSqlError;
use sqlx::AnyPool;
use sqlx::Row;
use std::collections::HashMap;

/// Generate a Mermaid ER diagram for all tables in a database.
pub async fn generate_mermaid_er(
    pool: &AnyPool,
    backend: DbBackend,
) -> Result<String, McpSqlError> {
    // Get all tables
    let table_rows = dialect::list_tables(pool, backend).await?;
    let table_names: Vec<String> = table_rows
        .iter()
        .filter_map(|r| r.get("table_name").and_then(|v| v.as_str()).map(String::from))
        .collect();

    if table_names.is_empty() {
        return Ok("erDiagram\n    %% No tables found".to_string());
    }

    let mut diagram = String::from("erDiagram\n");

    // FK relationships: (from_table, to_table, from_col, to_col)
    let mut relationships: Vec<(String, String, String, String)> = Vec::new();

    // Describe each table
    for table in &table_names {
        let columns = dialect::describe_table(pool, backend, table).await?;
        diagram.push_str(&format!("    {} {{\n", table));
        for col in &columns {
            let name = col.get("column_name").and_then(|v| v.as_str()).unwrap_or("?");
            let dtype = col.get("data_type").and_then(|v| v.as_str()).unwrap_or("?");
            let is_pk = col.get("is_primary_key")
                .and_then(|v| v.as_bool())
                .or_else(|| col.get("is_primary_key").and_then(|v| v.as_str()).map(|s| s == "YES" || s == "true"))
                .unwrap_or(false);
            let fk = col.get("foreign_key").and_then(|v| v.as_str());

            let mut suffix = String::new();
            if is_pk {
                suffix.push_str(" PK");
            }
            if fk.is_some() {
                suffix.push_str(" FK");
            }
            diagram.push_str(&format!("        {} {}{}\n", dtype.to_uppercase(), name, suffix));

            // Track FK relationships
            if let Some(fk_ref) = fk {
                if let Some((ref_table, _ref_col)) = fk_ref.split_once('.') {
                    relationships.push((
                        table.clone(),
                        ref_table.to_string(),
                        name.to_string(),
                        _ref_col.to_string(),
                    ));
                }
            }
        }
        diagram.push_str("    }\n");
    }

    // Add relationships
    // Deduplicate by (from_table, to_table) pair
    let mut seen: HashMap<(String, String), String> = HashMap::new();
    for (from, to, from_col, _to_col) in &relationships {
        seen.entry((from.clone(), to.clone()))
            .or_insert_with(|| from_col.clone());
    }
    for ((from, to), _col) in &seen {
        diagram.push_str(&format!("    {} ||--o{{ {} : \"\"\n", to, from));
    }

    Ok(diagram)
}
```

**Step 2: Add module to `src/lib.rs`**

```rust
pub mod db;
pub mod demo;
pub mod error;
pub mod schema;
pub mod server;
```

**Step 3: Add tool handler to `src/server.rs`**

Add the tool handler inside the `#[tool_router]` impl block:

```rust
    #[tool(
        name = "show_schema",
        description = "Show a Mermaid ER diagram of all tables and their relationships in the database"
    )]
    async fn show_schema(
        &self,
        Parameters(params): Parameters<DatabaseParam>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let diagram = crate::schema::generate_mermaid_er(&entry.pool, entry.backend)
            .await
            .map_err(|e| self.err(e))?;
        Ok(CallToolResult::success(vec![Content::text(diagram)]))
    }
```

**Step 4: Write test**

Add to `tests/sqlite_integration.rs`:

```rust
#[tokio::test]
async fn test_show_schema_mermaid() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    common::setup_test_schema(&pool).await;

    let diagram = mcp_sql::schema::generate_mermaid_er(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
    )
    .await
    .unwrap();

    assert!(diagram.starts_with("erDiagram"), "should start with erDiagram");
    assert!(diagram.contains("users"), "should contain users table");
    assert!(diagram.contains("posts"), "should contain posts table");
    assert!(diagram.contains("PK"), "should mark primary keys");
    assert!(diagram.contains("FK"), "should mark foreign keys");
    // FK relationship line
    assert!(diagram.contains("||--o{"), "should contain relationship lines");
}

#[tokio::test]
async fn test_show_schema_empty_database() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    // Don't create schema — empty database

    let diagram = mcp_sql::schema::generate_mermaid_er(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
    )
    .await
    .unwrap();

    assert!(diagram.contains("No tables found"));
}
```

**Step 5: Run tests**

```bash
cd /home/mehdi/projects/mcp-sql && cargo test
```

**Step 6: Commit**

```bash
git add src/schema.rs src/lib.rs src/server.rs tests/sqlite_integration.rs
git commit -m "feat: add show_schema tool with Mermaid ER diagram"
```

---

### Task 4: Fix SQLite row counts in `list_tables`

**Files:**
- Modify: `src/db/dialect.rs`
- Test: `tests/sqlite_integration.rs`

**Step 1: Replace SQLite `list_tables` query**

In `src/db/dialect.rs`, replace the SQLite arm in `list_tables()` (the one that hardcodes row_count=0). The new approach queries each table's count individually since SQLite has no stats table:

```rust
DbBackend::Sqlite => {
    // Get table names first
    let name_rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for row in &name_rows {
        let name: String = row.try_get("name")?;
        // Count rows with a timeout — fall back to 0 for very large tables
        let count: i64 = match tokio::time::timeout(
            std::time::Duration::from_secs(1),
            sqlx::query_as::<_, (i64,)>(&format!(
                "SELECT COUNT(*) FROM \"{}\"",
                name.replace('"', "\"\"")
            ))
            .fetch_one(pool),
        )
        .await
        {
            Ok(Ok((c,))) => c,
            _ => 0,
        };
        results.push(serde_json::json!({
            "table_name": name,
            "row_count": count,
        }));
    }
    Ok(results)
}
```

This replaces the single-query approach. The return type of `list_tables` is already `Vec<Value>`, so it's compatible.

**Step 2: Update test**

Modify the existing `test_list_tables` in `tests/sqlite_integration.rs` to assert non-zero row counts:

Find the existing test and update the row count assertion:

```rust
#[tokio::test]
async fn test_list_tables() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    common::setup_test_schema(&pool).await;

    let tables = mcp_sql::db::dialect::list_tables(&pool, mcp_sql::db::DbBackend::Sqlite)
        .await
        .unwrap();

    let names: Vec<&str> = tables
        .iter()
        .filter_map(|t| t.get("table_name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"users"));
    assert!(names.contains(&"posts"));

    // Verify row counts are now accurate (not hardcoded 0)
    let users_count = tables
        .iter()
        .find(|t| t.get("table_name").and_then(|v| v.as_str()) == Some("users"))
        .and_then(|t| t.get("row_count").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    assert_eq!(users_count, 2, "users table should have 2 rows");

    let posts_count = tables
        .iter()
        .find(|t| t.get("table_name").and_then(|v| v.as_str()) == Some("posts"))
        .and_then(|t| t.get("row_count").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    assert_eq!(posts_count, 1, "posts table should have 1 row");
}
```

**Step 3: Run tests**

```bash
cd /home/mehdi/projects/mcp-sql && cargo test
```

**Step 4: Commit**

```bash
git add src/db/dialect.rs tests/sqlite_integration.rs
git commit -m "fix: return accurate row counts for SQLite tables"
```

---

### Task 5: Add index information to `describe_table`

**Files:**
- Modify: `src/db/dialect.rs`
- Test: `tests/sqlite_integration.rs`

**Step 1: Add `list_indexes()` function to `src/db/dialect.rs`**

Add after `describe_table_mysql()`:

```rust
/// Returns index information for a table.
pub async fn list_indexes(
    pool: &AnyPool,
    backend: DbBackend,
    table: &str,
) -> Result<Vec<Value>, McpSqlError> {
    let table = sanitize_identifier(table)?;

    match backend {
        DbBackend::Sqlite => {
            let index_rows = sqlx::query(&format!("PRAGMA index_list(\"{}\")", table.replace('"', "\"\"")))
                .fetch_all(pool)
                .await?;

            let mut indexes = Vec::new();
            for row in &index_rows {
                let name: String = row.try_get("name")?;
                let unique: bool = row.try_get::<i32, _>("unique").map(|v| v == 1).unwrap_or(false);

                let col_rows = sqlx::query(&format!("PRAGMA index_info(\"{}\")", name.replace('"', "\"\"")))
                    .fetch_all(pool)
                    .await?;
                let columns: Vec<String> = col_rows
                    .iter()
                    .filter_map(|r| r.try_get::<String, _>("name").ok())
                    .collect();

                indexes.push(serde_json::json!({
                    "index_name": name,
                    "columns": columns,
                    "unique": unique,
                }));
            }
            Ok(indexes)
        }
        DbBackend::Postgres => {
            let rows = sqlx::query(
                "SELECT indexname, indexdef FROM pg_indexes WHERE tablename = $1 ORDER BY indexname",
            )
            .bind(&table)
            .fetch_all(pool)
            .await?;

            let mut indexes = Vec::new();
            for row in &rows {
                let name: String = row.try_get("indexname")?;
                let def: String = row.try_get("indexdef")?;
                let unique = def.to_uppercase().contains("UNIQUE");
                indexes.push(serde_json::json!({
                    "index_name": name,
                    "definition": def,
                    "unique": unique,
                }));
            }
            Ok(indexes)
        }
        DbBackend::Mysql => {
            let rows = sqlx::query(
                "SELECT INDEX_NAME, COLUMN_NAME, NON_UNIQUE \
                 FROM information_schema.STATISTICS \
                 WHERE TABLE_NAME = ? AND TABLE_SCHEMA = DATABASE() \
                 ORDER BY INDEX_NAME, SEQ_IN_INDEX",
            )
            .bind(&table)
            .fetch_all(pool)
            .await?;

            // Group columns by index name
            let mut index_map: std::collections::HashMap<String, (Vec<String>, bool)> =
                std::collections::HashMap::new();
            for row in &rows {
                let name: String = row.try_get("INDEX_NAME")?;
                let col: String = row.try_get("COLUMN_NAME")?;
                let non_unique: i32 = row.try_get("NON_UNIQUE")?;
                let entry = index_map.entry(name).or_insert_with(|| (Vec::new(), non_unique == 0));
                entry.0.push(col);
            }

            let indexes = index_map
                .into_iter()
                .map(|(name, (columns, unique))| {
                    serde_json::json!({
                        "index_name": name,
                        "columns": columns,
                        "unique": unique,
                    })
                })
                .collect();
            Ok(indexes)
        }
    }
}
```

**Step 2: Add `list_indexes` tool handler to `src/server.rs`**

Add param type:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
struct ListIndexesParams {
    /// Table name
    table: String,
    /// Database name (optional if only one database is connected)
    database: Option<String>,
}
```

Add tool handler:

```rust
    #[tool(
        name = "list_indexes",
        description = "List all indexes on a table with column names and uniqueness"
    )]
    async fn list_indexes(
        &self,
        Parameters(params): Parameters<ListIndexesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;
        let indexes = dialect::list_indexes(&entry.pool, entry.backend, &params.table)
            .await
            .map_err(|e| self.err(e))?;
        let json = serde_json::to_string_pretty(&indexes).unwrap_or_default();
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
```

**Step 3: Write test**

Add to `tests/sqlite_integration.rs`:

```rust
#[tokio::test]
async fn test_list_indexes() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    common::setup_test_schema(&pool).await;

    // Create an explicit index
    sqlx::query("CREATE INDEX idx_users_email ON users(email)")
        .execute(&pool)
        .await
        .unwrap();

    let indexes = mcp_sql::db::dialect::list_indexes(
        &pool,
        mcp_sql::db::DbBackend::Sqlite,
        "users",
    )
    .await
    .unwrap();

    assert!(!indexes.is_empty(), "should have at least one index");
    let idx = indexes.iter().find(|i| {
        i.get("index_name").and_then(|v| v.as_str()) == Some("idx_users_email")
    });
    assert!(idx.is_some(), "should find idx_users_email");
    let cols = idx.unwrap().get("columns").and_then(|v| v.as_array()).unwrap();
    assert!(cols.iter().any(|c| c.as_str() == Some("email")));
}
```

**Step 4: Run tests**

```bash
cd /home/mehdi/projects/mcp-sql && cargo test
```

**Step 5: Commit**

```bash
git add src/db/dialect.rs src/server.rs tests/sqlite_integration.rs
git commit -m "feat: add list_indexes tool with per-database index introspection"
```

---

### Task 6: Add `query_dry_run` tool

**Files:**
- Modify: `src/server.rs`
- Test: `tests/sqlite_integration.rs`

**Step 1: Add tool handler to `src/server.rs`**

```rust
    #[tool(
        name = "query_dry_run",
        description = "Validate a SQL query without executing it. Returns the query plan and any warnings."
    )]
    async fn query_dry_run(
        &self,
        Parameters(params): Parameters<QueryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let entry = self.db.resolve(params.database.as_deref()).map_err(|e| self.err(e))?;

        // Use EXPLAIN to validate without executing
        let explain_sql = format!(
            "{}{}",
            dialect::explain_prefix(entry.backend),
            params.sql
        );

        match sqlx::query(&explain_sql).fetch_all(&entry.pool).await {
            Ok(rows) => {
                let plan: Vec<Value> = rows.iter().map(row_to_json).collect();
                let result = serde_json::json!({
                    "valid": true,
                    "query_plan": plan,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                let result = serde_json::json!({
                    "valid": false,
                    "error": e.to_string(),
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap_or_default(),
                )]))
            }
        }
    }
```

Note: this returns `valid: false` as content (not an error), because a dry run that finds invalid SQL is a successful tool call — it did its job.

**Step 2: Add import for `row_to_json` if not already imported**

In `src/server.rs`, ensure `use crate::db::convert::row_to_json;` is present (it may already be imported for the `query` handler).

**Step 3: Write test**

Add to `tests/sqlite_integration.rs`:

```rust
#[tokio::test]
async fn test_query_dry_run_valid() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    common::setup_test_schema(&pool).await;

    let explain_sql = format!(
        "{}SELECT * FROM users WHERE name = 'Alice'",
        mcp_sql::db::dialect::explain_prefix(mcp_sql::db::DbBackend::Sqlite),
    );
    let rows = sqlx::query(&explain_sql).fetch_all(&pool).await;
    assert!(rows.is_ok(), "valid SQL should produce a query plan");
}

#[tokio::test]
async fn test_query_dry_run_invalid() {
    sqlx::any::install_default_drivers();
    let pool = common::create_test_pool().await;
    common::setup_test_schema(&pool).await;

    let explain_sql = format!(
        "{}SELECT * FROM nonexistent_table",
        mcp_sql::db::dialect::explain_prefix(mcp_sql::db::DbBackend::Sqlite),
    );
    let result = sqlx::query(&explain_sql).fetch_all(&pool).await;
    assert!(result.is_err(), "invalid SQL should produce an error");
}
```

**Step 4: Run tests**

```bash
cd /home/mehdi/projects/mcp-sql && cargo test
```

**Step 5: Commit**

```bash
git add src/server.rs tests/sqlite_integration.rs
git commit -m "feat: add query_dry_run tool for SQL validation"
```

---

### Task 7: Update README and version bump

**Files:**
- Modify: `README.md`
- Modify: `Cargo.toml`

**Step 1: Update `Cargo.toml` version**

Change `version = "0.1.0"` to `version = "0.2.0"`.

**Step 2: Update `README.md`**

Add the new tools to the tools table. Add `--demo` to usage examples. Add Claude Code one-liner. Update the tools count.

Key additions to the usage section:

```markdown
# Demo mode — try it instantly with sample data
mcp-sql --demo
```

Add new rows to the tools table:

```markdown
| `show_schema`       | `database?`                   | Mermaid ER diagram of all tables and relationships  |
| `show_create_table` | `table`, `database?`          | CREATE TABLE DDL statement                          |
| `query_dry_run`     | `sql`, `database?`            | Validate SQL and show query plan without executing  |
| `list_indexes`      | `table`, `database?`          | Index names, columns, and uniqueness constraints    |
```

Add Claude Code setup section:

```markdown
## Quick Setup (Claude Code)

```bash
claude mcp add mcp-sql -- mcp-sql --url sqlite:mydb.db
```
```

**Step 3: Run clippy + tests**

```bash
cd /home/mehdi/projects/mcp-sql && cargo clippy -- -D warnings && cargo test
```

**Step 4: Commit**

```bash
git add Cargo.toml README.md
git commit -m "release: v0.2.0 — 4 new tools, --demo flag, SQLite row counts"
```

---

### Task 8: Tag, push, and publish

**Step 1: Push**

```bash
cd /home/mehdi/projects/mcp-sql && git push origin main
```

**Step 2: Tag**

```bash
git tag v0.2.0
git push origin v0.2.0
```

**Step 3: Publish to crates.io**

```bash
cargo publish
```

**Step 4: Verify**

```bash
cargo search mcp-sql
```

Expected: `mcp-sql = "0.2.0"`
