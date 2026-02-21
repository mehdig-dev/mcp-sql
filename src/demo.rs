use sqlx::any::AnyPoolOptions;
use sqlx::AnyPool;

/// Creates an in-memory SQLite database with sample tables for demo mode.
pub async fn create_demo_database() -> Result<AnyPool, sqlx::Error> {
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

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
