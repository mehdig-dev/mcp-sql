use anyhow::{bail, Result};
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
    #[arg(long = "url")]
    urls: Vec<String>,

    /// Read a database URL from an environment variable (repeatable).
    /// Example: --url-env DATABASE_URL
    #[arg(long = "url-env")]
    url_envs: Vec<String>,

    /// Allow write operations (INSERT, UPDATE, DELETE, CREATE, DROP).
    /// By default, only read-only queries are permitted.
    #[arg(long)]
    allow_write: bool,

    /// Maximum number of rows returned per query (default: 100)
    #[arg(long, default_value = "100")]
    row_limit: u32,

    /// Query timeout in seconds (default: 30)
    #[arg(long, default_value = "30")]
    query_timeout: u64,
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

    // Collect URLs from --url and --url-env
    let mut all_urls = cli.urls.clone();

    for env_name in &cli.url_envs {
        match std::env::var(env_name) {
            Ok(url) => {
                tracing::info!(env = env_name, "Read database URL from environment variable");
                all_urls.push(url);
            }
            Err(_) => {
                bail!("Environment variable '{env_name}' is not set");
            }
        }
    }

    if all_urls.is_empty() {
        bail!("No database URLs provided. Use --url or --url-env to specify at least one database.");
    }

    tracing::info!(
        databases = all_urls.len(),
        allow_write = cli.allow_write,
        row_limit = cli.row_limit,
        query_timeout = cli.query_timeout,
        "Starting mcp-sql server"
    );

    let db = db::DatabaseManager::new(&all_urls).await?;

    tracing::info!(
        databases = ?db.databases.iter().map(|d| format!("{}({})", d.name, d.backend.name())).collect::<Vec<_>>(),
        "Connected to databases"
    );

    let service = server::McpSqlServer::new(db, cli.allow_write, cli.row_limit, cli.query_timeout);
    let running = service.serve(stdio()).await?;
    running.waiting().await?;

    Ok(())
}
