use sqlx::any::AnyPoolOptions;
use sqlx::AnyPool;

pub async fn create_test_pool() -> AnyPool {
    AnyPoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite pool")
}

pub async fn setup_test_schema(pool: &AnyPool) {
    sqlx::query(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            email TEXT,
            active INTEGER NOT NULL DEFAULT 1
        )",
    )
    .execute(pool)
    .await
    .expect("Failed to create users table");

    sqlx::query(
        "CREATE TABLE posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            body TEXT,
            FOREIGN KEY (user_id) REFERENCES users(id)
        )",
    )
    .execute(pool)
    .await
    .expect("Failed to create posts table");

    sqlx::query("INSERT INTO users (name, email, active) VALUES ('Alice', 'alice@example.com', 1)")
        .execute(pool)
        .await
        .expect("Failed to insert Alice");

    sqlx::query("INSERT INTO users (name, email, active) VALUES ('Bob', 'bob@example.com', 0)")
        .execute(pool)
        .await
        .expect("Failed to insert Bob");

    sqlx::query("INSERT INTO posts (user_id, title, body) VALUES (1, 'Hello World', 'First post!')")
        .execute(pool)
        .await
        .expect("Failed to insert post");
}
