pub mod convert;
pub mod dialect;

use sqlx::any::AnyPoolOptions;
use sqlx::AnyPool;

use crate::error::McpSqlError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbBackend {
    Postgres,
    Sqlite,
    Mysql,
}

impl DbBackend {
    pub fn from_url(url: &str) -> Result<Self, McpSqlError> {
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Ok(DbBackend::Postgres)
        } else if url.starts_with("sqlite:") {
            Ok(DbBackend::Sqlite)
        } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
            Ok(DbBackend::Mysql)
        } else {
            Err(McpSqlError::Other(format!(
                "Unsupported database URL scheme: {url}"
            )))
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DbBackend::Postgres => "postgres",
            DbBackend::Sqlite => "sqlite",
            DbBackend::Mysql => "mysql",
        }
    }
}

#[derive(Clone)]
pub struct DatabaseEntry {
    pub name: String,
    pub pool: AnyPool,
    pub backend: DbBackend,
    pub url_redacted: String,
}

#[derive(Clone)]
pub struct DatabaseManager {
    pub databases: Vec<DatabaseEntry>,
}

impl DatabaseManager {
    pub async fn new(urls: &[String]) -> Result<Self, McpSqlError> {
        let mut databases = Vec::with_capacity(urls.len());

        for url in urls {
            let backend = DbBackend::from_url(url)?;
            let name = extract_db_name(url, backend);

            let pool = AnyPoolOptions::new()
                .max_connections(5)
                .connect(url)
                .await?;

            databases.push(DatabaseEntry {
                name,
                pool,
                backend,
                url_redacted: redact_url(url),
            });
        }

        Ok(Self { databases })
    }

    /// Resolve which database to use. If `database` param is None and there's
    /// exactly one DB, use it. Otherwise require an explicit name.
    pub fn resolve(&self, database: Option<&str>) -> Result<&DatabaseEntry, McpSqlError> {
        match database {
            Some(name) => self
                .databases
                .iter()
                .find(|d| d.name == name)
                .ok_or_else(|| {
                    let available: Vec<&str> = self.databases.iter().map(|d| d.name.as_str()).collect();
                    McpSqlError::DatabaseNotFound(format!(
                        "'{name}' not found. Available: {}",
                        available.join(", ")
                    ))
                }),
            None => {
                if self.databases.len() == 1 {
                    Ok(&self.databases[0])
                } else {
                    Err(McpSqlError::AmbiguousDatabase)
                }
            }
        }
    }
}

/// Extract a human-friendly name from the URL.
fn extract_db_name(url: &str, backend: DbBackend) -> String {
    match backend {
        DbBackend::Sqlite => {
            let path = url.strip_prefix("sqlite:").unwrap_or(url);
            if path == ":memory:" || path.is_empty() {
                return "memory".to_string();
            }
            // Use the filename without extension
            std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("sqlite")
                .to_string()
        }
        DbBackend::Postgres | DbBackend::Mysql => {
            // Parse as URL, extract the database name from the path
            if let Ok(parsed) = url::Url::parse(url) {
                let path = parsed.path().trim_start_matches('/');
                if !path.is_empty() {
                    return path.to_string();
                }
            }
            backend.name().to_string()
        }
    }
}

/// Redact password from a database URL.
fn redact_url(url: &str) -> String {
    if let Ok(mut parsed) = url::Url::parse(url) {
        if parsed.password().is_some() {
            let _ = parsed.set_password(Some("****"));
        }
        parsed.to_string()
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_from_url() {
        assert_eq!(
            DbBackend::from_url("postgres://localhost/mydb").unwrap(),
            DbBackend::Postgres
        );
        assert_eq!(
            DbBackend::from_url("postgresql://localhost/mydb").unwrap(),
            DbBackend::Postgres
        );
        assert_eq!(
            DbBackend::from_url("sqlite:test.db").unwrap(),
            DbBackend::Sqlite
        );
        assert_eq!(
            DbBackend::from_url("sqlite::memory:").unwrap(),
            DbBackend::Sqlite
        );
        assert_eq!(
            DbBackend::from_url("mysql://localhost/mydb").unwrap(),
            DbBackend::Mysql
        );
        assert!(DbBackend::from_url("oracle://localhost/mydb").is_err());
    }

    #[test]
    fn test_extract_db_name() {
        assert_eq!(
            extract_db_name("postgres://user:pass@localhost/mydb", DbBackend::Postgres),
            "mydb"
        );
        assert_eq!(
            extract_db_name("sqlite:test.db", DbBackend::Sqlite),
            "test"
        );
        assert_eq!(
            extract_db_name("sqlite::memory:", DbBackend::Sqlite),
            "memory"
        );
        assert_eq!(
            extract_db_name("mysql://user:pass@localhost/app", DbBackend::Mysql),
            "app"
        );
    }

    #[test]
    fn test_redact_url() {
        assert_eq!(
            redact_url("postgres://user:secret@localhost/mydb"),
            "postgres://user:****@localhost/mydb"
        );
        assert_eq!(
            redact_url("postgres://user@localhost/mydb"),
            "postgres://user@localhost/mydb"
        );
        assert_eq!(redact_url("sqlite:test.db"), "sqlite:test.db");
    }
}
