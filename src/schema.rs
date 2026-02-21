use std::collections::HashMap;

use sqlx::AnyPool;

use crate::db::dialect;
use crate::db::DbBackend;
use crate::error::McpSqlError;

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
            let name = col
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| col.get("column_name").and_then(|v| v.as_str()))
                .unwrap_or("?");
            let dtype = col
                .get("type")
                .and_then(|v| v.as_str())
                .or_else(|| col.get("data_type").and_then(|v| v.as_str()))
                .unwrap_or("?");
            let is_pk = col
                .get("primary_key")
                .and_then(|v| v.as_str())
                .map(|s| s == "YES")
                .or_else(|| col.get("is_primary_key").and_then(|v| v.as_bool()))
                .unwrap_or(false);
            let fk = col.get("foreign_key").and_then(|v| v.as_str());

            let mut suffix = String::new();
            if is_pk {
                suffix.push_str(" PK");
            }
            if fk.is_some() {
                suffix.push_str(" FK");
            }
            // Mermaid ER format: TYPE name CONSTRAINT
            // Type names cannot contain spaces, so replace spaces with underscores.
            diagram.push_str(&format!(
                "        {} {}{}\n",
                dtype.to_uppercase().replace(' ', "_"),
                name,
                suffix
            ));

            // Track FK relationships
            if let Some(fk_ref) = fk {
                if let Some((ref_table, ref_col)) = fk_ref.split_once('.') {
                    relationships.push((
                        table.clone(),
                        ref_table.to_string(),
                        name.to_string(),
                        ref_col.to_string(),
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
    for (from, to) in seen.keys() {
        diagram.push_str(&format!("    {} ||--o{{ {} : \"\"\n", to, from));
    }

    Ok(diagram)
}
