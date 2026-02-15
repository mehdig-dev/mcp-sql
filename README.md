# mcp-sql

MCP server that lets LLMs query PostgreSQL, SQLite, and MySQL databases. Single binary, read-only by default, multi-database.

## Install

```bash
cargo install mcp-sql
```

## Usage

```bash
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
| `list_tables` | List tables with approximate row counts |
| `describe_table` | Column details: name, type, nullable, default, primary key |
| `query` | Execute SQL and return results as JSON |
| `explain` | Show query execution plan |

All tools accept an optional `database` parameter when multiple databases are connected. If only one database is connected, it's used automatically.

## Safety

- **Read-only by default** — only `SELECT`, `WITH`, `SHOW`, `PRAGMA`, and `EXPLAIN` queries are allowed
- **Row limit enforced** — `LIMIT` is injected if not present (default: 100)
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
