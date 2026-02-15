use serde_json::Value;
use sqlx::any::AnyRow;
use sqlx::{Column, Row, TypeInfo, ValueRef};

/// Convert an AnyRow to a JSON object by inspecting column type info names.
pub fn row_to_json(row: &AnyRow) -> Value {
    let mut obj = serde_json::Map::new();

    for col in row.columns() {
        let name = col.name().to_string();
        let ordinal = col.ordinal();
        let type_name = col.type_info().name().to_uppercase();

        let value = decode_column(row, ordinal, &type_name);
        obj.insert(name, value);
    }

    Value::Object(obj)
}

fn decode_column(row: &AnyRow, ordinal: usize, type_name: &str) -> Value {
    // Try NULL first
    if let Ok(v) = row.try_get_raw(ordinal) {
        if v.is_null() {
            return Value::Null;
        }
    }

    match type_name {
        // Boolean types
        "BOOL" | "BOOLEAN" => {
            if let Ok(v) = row.try_get::<bool, _>(ordinal) {
                return Value::Bool(v);
            }
        }

        // Integer types
        "INT2" | "SMALLINT" | "TINYINT" => {
            if let Ok(v) = row.try_get::<i16, _>(ordinal) {
                return Value::Number(v.into());
            }
        }
        "INT" | "INT4" | "INTEGER" | "MEDIUMINT" => {
            if let Ok(v) = row.try_get::<i32, _>(ordinal) {
                return Value::Number(v.into());
            }
        }
        "INT8" | "BIGINT" => {
            if let Ok(v) = row.try_get::<i64, _>(ordinal) {
                return Value::Number(v.into());
            }
        }

        // Float types
        "FLOAT4" | "REAL" | "FLOAT" => {
            if let Ok(v) = row.try_get::<f64, _>(ordinal) {
                return serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .unwrap_or(Value::Null);
            }
        }
        "FLOAT8" | "DOUBLE" | "DOUBLE PRECISION" | "NUMERIC" | "DECIMAL" => {
            if let Ok(v) = row.try_get::<f64, _>(ordinal) {
                return serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .unwrap_or(Value::Null);
            }
        }

        // Blob types
        "BYTEA" | "BLOB" | "BINARY" | "VARBINARY" | "LONGBLOB" | "MEDIUMBLOB" | "TINYBLOB" => {
            if let Ok(v) = row.try_get::<Vec<u8>, _>(ordinal) {
                return Value::String(format!("(blob: {} bytes)", v.len()));
            }
        }

        // Text/string types and everything else — fall through to the fallback below
        _ => {}
    }

    // Fallback chain: try integer, float, bool, then string
    if let Ok(v) = row.try_get::<i64, _>(ordinal) {
        return Value::Number(v.into());
    }
    if let Ok(v) = row.try_get::<f64, _>(ordinal) {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return Value::Number(n);
        }
    }
    if let Ok(v) = row.try_get::<bool, _>(ordinal) {
        return Value::Bool(v);
    }
    if let Ok(v) = row.try_get::<String, _>(ordinal) {
        return Value::String(v);
    }

    Value::Null
}
