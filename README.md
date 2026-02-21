<div align="center">
  <img src="mascot.png" alt="mcp-sql owl mascot" width="200">

  # mcp-sql

  MCP server that lets LLMs query PostgreSQL, SQLite, and MySQL databases. Single binary, read-only by default, multi-database.
</div>

## Install

```bash
cargo install mcp-sql
```

## Usage

```bash
# Demo mode — try it instantly with sample data
mcp-sql --demo

# Single database
mcp-sql --url postgres://user:pass@localhost/mydb

# SQLite
mcp-sql --url sqlite:path/to/db.sqlite

# Multiple databases
mcp-sql --url postgres://localhost/app --url sqlite:analytics.db

# Enable write operations
mcp-sql --url sqlite:local.db --allow-write

# Custom row limit
mcp-sql --url mysql://user:pass@localhost/shop --row-limit 500

# Read URL from environment variable
mcp-sql --url-env DATABASE_URL

# Mix --url and --url-env
mcp-sql --url sqlite:local.db --url-env PROD_DB_URL

# Custom query timeout (default: 30s)
mcp-sql --url sqlite:local.db --query-timeout 60
```

## Configuration

### Claude Code

```bash
claude mcp add sql -- mcp-sql --url sqlite:path/to/your.db
```

Or edit `~/.claude.json` directly:

```json
{
  "mcpServers": {
    "sql": {
      "type": "stdio",
      "command": "mcp-sql",
      "args": ["--url", "sqlite:path/to/your.db"]
    }
  }
}
```

### Claude Desktop

Add to your Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "sql": {
      "command": "mcp-sql",
      "args": [
        "--url", "postgres://user:pass@localhost/mydb",
        "--url", "sqlite:analytics.db"
      ]
    }
  }
}
```

### Cursor / VS Code

Add to your MCP config (`.cursor/mcp.json` or equivalent):

```json
{
  "mcpServers": {
    "sql": {
      "command": "mcp-sql",
      "args": ["--url", "sqlite:path/to/your.db"]
    }
  }
}
```

## Tools

| Tool | Description |
|------|-------------|
| `list_databases` | Show all connected databases with name and type |
| `list_tables` | List tables with row counts |
| `describe_table` | Column details: name, type, nullable, default, primary key, foreign key |
| `show_create_table` | Show the CREATE TABLE DDL statement for a table |
| `show_schema` | Mermaid ER diagram of all tables and their relationships |
| `list_indexes` | Index names, columns, and uniqueness constraints |
| `sample_data` | Return sample rows from a table as JSON (no SQL needed) |
| `query` | Execute SQL and return results as JSON |
| `explain` | Show query execution plan |
| `query_dry_run` | Validate SQL and show query plan without executing |

All tools accept an optional `database` parameter when multiple databases are connected. If only one database is connected, it's used automatically.

## CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `--url` | — | Database connection URL (repeatable) |
| `--url-env` | — | Read database URL from an environment variable (repeatable) |
| `--demo` | `false` | Start with a demo SQLite database pre-loaded with sample data |
| `--allow-write` | `false` | Enable write operations (INSERT, UPDATE, DELETE, CREATE, DROP) |
| `--row-limit` | `100` | Maximum rows returned per query |
| `--query-timeout` | `30` | Query timeout in seconds |

At least one `--url` or `--url-env` is required (unless using `--demo`).

## Safety

- **Read-only by default** — only `SELECT`, `WITH`, `SHOW`, `PRAGMA`, and `EXPLAIN` queries are allowed
- **Row limit enforced** — `LIMIT` is injected if not present (default: 100)
- **Query timeout** — queries are killed after the configured timeout (default: 30s)
- **Credentials redacted** — passwords are masked in `list_databases` output
- **PostgreSQL/MySQL** — additionally uses `SET TRANSACTION READ ONLY` for database-level enforcement

Pass `--allow-write` to enable `INSERT`, `UPDATE`, `DELETE`, `CREATE`, and `DROP` operations.

## Supported Databases

| Database | URL Scheme | Notes |
|----------|-----------|-------|
| PostgreSQL | `postgres://` or `postgresql://` | Full support |
| SQLite | `sqlite:path` or `sqlite::memory:` | Full support |
| MySQL | `mysql://` or `mariadb://` | Full support |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
