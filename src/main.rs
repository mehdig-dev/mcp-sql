use anyhow::Result;
use clap::Parser;
use mcp_sql::{db, server};
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

/// MCP server for SQL databases — lets LLMs query PostgreSQL, SQLite, and MySQL
#[derive(Parser)]
#[command(name = "mcp-sql", version, about)]
struct Cli {
    /// Database connection URL (repeatable for multiple databases).
    /// Scheme determines DB type: postgres://, sqlite:, mysql://
    #[arg(long = "url", required = true)]
    urls: Vec<String>,

    /// Allow write operations (INSERT, UPDATE, DELETE, CREATE, DROP).
    /// By default, only read-only queries are permitted.
    #[arg(long)]
    allow_write: bool,

    /// Maximum number of rows returned per query (default: 100)
    #[arg(long, default_value = "100")]
    row_limit: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    // Install sqlx's runtime drivers for all supported databases
    sqlx::any::install_default_drivers();

    tracing::info!(
        databases = cli.urls.len(),
        allow_write = cli.allow_write,
        row_limit = cli.row_limit,
        "Starting mcp-sql server"
    );

    let db = db::DatabaseManager::new(&cli.urls).await?;

    tracing::info!(
        databases = ?db.databases.iter().map(|d| format!("{}({})", d.name, d.backend.name())).collect::<Vec<_>>(),
        "Connected to databases"
    );

    let service = server::McpSqlServer::new(db, cli.allow_write, cli.row_limit);
    let running = service.serve(stdio()).await?;
    running.waiting().await?;

    Ok(())
}
