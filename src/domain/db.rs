//! The `db` capability's rules (ADR-2610071219): each package's own SQLite database, what an app
//! may ask of it, and the limits. Pure: the database itself is behind the `Database` port.

use serde_json::Value;

/// The most rows one call returns.
pub const MAX_ROWS_PER_CALL: usize = 1000;
/// The largest database a package may keep.
pub const MAX_DB_BYTES: u64 = 1024 * 1024 * 1024;
/// The longest one statement may run, in milliseconds.
pub const MAX_STATEMENT_MS: u64 = 10_000;
/// The most rows one Splunk search loads into a table.
pub const MAX_LOADED_ROWS: usize = 1_000_000;
const MAX_SQL_CHARS: usize = 20_000;
const MAX_WHERE_CHARS: usize = 2_000;
const MAX_PARAMS: usize = 100;

/// A table or column name an app may use: `[A-Za-z_][A-Za-z0-9_]{0,63}`.
pub fn valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && s.len() <= 64 && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A name, quoted for SQL. Only ever called with a valid identifier.
pub fn quoted(ident: &str) -> String {
    format!("\"{ident}\"")
}

/// One statement an app may send: within the length limit, with no second statement after it.
/// The database's authorizer decides what the statement may touch.
pub fn check_statement(sql: &str) -> Result<String, String> {
    let sql = sql.trim().trim_end_matches(';').trim_end();
    if sql.is_empty() {
        return Err("the statement is empty".into());
    }
    if sql.chars().count() > MAX_SQL_CHARS {
        return Err(format!("a statement may be at most {MAX_SQL_CHARS} characters"));
    }
    if sql.contains(';') {
        return Err("send one statement at a time (put values in parameters, not in the text)".into());
    }
    Ok(sql.to_string())
}

/// Parameters for a statement: plain JSON values, at most a hundred.
pub fn check_params(params: &Value) -> Result<Vec<Value>, String> {
    let list = match params {
        Value::Null => Vec::new(),
        Value::Array(a) => a.clone(),
        _ => return Err("params must be a list".into()),
    };
    if list.len() > MAX_PARAMS {
        return Err(format!("at most {MAX_PARAMS} parameters"));
    }
    if list.iter().any(|v| v.is_object() || v.is_array()) {
        return Err("a parameter must be a number, text, true/false or null".into());
    }
    Ok(list)
}

/// One page of a table: validated parts the host builds SQL from.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRequest {
    pub table: String,
    pub offset: u64,
    pub limit: u64,
    pub order_by: Option<String>,
    pub desc: bool,
    /// A condition with `?` placeholders, checked like a statement and run read-only.
    pub filter: Option<String>,
    pub params: Vec<Value>,
}

impl PageRequest {
    pub fn from_json(v: &Value) -> Result<PageRequest, String> {
        let table = v["table"].as_str().unwrap_or("");
        if !valid_ident(table) {
            return Err(format!("\"{table}\" is not a table name (letters, digits and _)"));
        }
        let offset = v["offset"].as_u64().unwrap_or(0);
        let limit = v["limit"].as_u64().unwrap_or(100).clamp(1, MAX_ROWS_PER_CALL as u64);
        let order_by = match v["orderBy"].as_str().filter(|s| !s.is_empty()) {
            Some(c) if valid_ident(c) => Some(c.to_string()),
            Some(c) => return Err(format!("\"{c}\" is not a column name")),
            None => None,
        };
        let filter = match v["where"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
            Some(w) if w.chars().count() > MAX_WHERE_CHARS => return Err(format!("a condition may be at most {MAX_WHERE_CHARS} characters")),
            Some(w) if w.contains(';') || w.contains("--") || w.contains("/*") => return Err("a condition may not hold ';' or comments".into()),
            Some(w) => Some(w.to_string()),
            None => None,
        };
        Ok(PageRequest { table: table.into(), offset, limit, order_by, desc: v["desc"].as_bool().unwrap_or(false), filter, params: check_params(&v["params"])? })
    }

    /// The statement for the rows, and the one for the count; both take `params`, the rows one
    /// then the limit and the offset.
    pub fn sql(&self) -> (String, String) {
        let from = format!("FROM {}{}", quoted(&self.table), self.filter.as_ref().map(|w| format!(" WHERE ({w})")).unwrap_or_default());
        let order = self.order_by.as_ref().map(|c| format!(" ORDER BY {} {}", quoted(c), if self.desc { "DESC" } else { "ASC" })).unwrap_or_default();
        (format!("SELECT * {from}{order} LIMIT ? OFFSET ?"), format!("SELECT count(*) {from}"))
    }
}

/// How a loaded column is declared: NUMERIC when every sampled value that is present reads as a
/// number, so sorting and comparing work on numbers; TEXT otherwise.
pub fn column_types(columns: usize, sample: &[Vec<Value>]) -> Vec<&'static str> {
    (0..columns)
        .map(|i| {
            let mut seen = false;
            let numeric = sample.iter().filter_map(|r| r.get(i)).all(|v| match v {
                Value::Null => true,
                Value::Number(_) => {
                    seen = true;
                    true
                }
                Value::String(s) if s.trim().is_empty() => true,
                Value::String(s) => {
                    seen = true;
                    s.trim().parse::<f64>().is_ok()
                }
                _ => false,
            });
            if numeric && seen {
                "NUMERIC"
            } else {
                "TEXT"
            }
        })
        .collect()
}

/// Columns for a new table: valid, distinct names made from whatever the data calls them.
pub fn column_names(fields: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        let mut name: String = f.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
        if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            name = format!("c_{name}");
        }
        name.truncate(60);
        let base = name.clone();
        let mut n = 2;
        while out.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        if name.is_empty() {
            name = format!("c{i}");
        }
        out.push(name);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identifiers() {
        assert!(valid_ident("search_1") && valid_ident("_x") && !valid_ident("1x") && !valid_ident("a-b") && !valid_ident("a\"b") && !valid_ident(""));
        assert!(!valid_ident(&"a".repeat(65)));
    }

    #[test]
    fn one_statement_only() {
        assert_eq!(check_statement("select 1;").unwrap(), "select 1");
        assert!(check_statement("select 1; drop table x").is_err());
        assert!(check_statement("  ").is_err());
        assert!(check_params(&json!([1, "a", null, true])).is_ok());
        assert!(check_params(&json!([[1]])).is_err());
        assert!(check_params(&json!({"a": 1})).is_err());
    }

    #[test]
    fn pages_are_built_from_checked_parts() {
        let p = PageRequest::from_json(&json!({"table": "search", "offset": 200, "limit": 5000, "orderBy": "n", "desc": true, "where": "status = ?", "params": [500]})).unwrap();
        assert_eq!(p.limit, 1000, "the limit is capped");
        let (rows, count) = p.sql();
        assert_eq!(rows, "SELECT * FROM \"search\" WHERE (status = ?) ORDER BY \"n\" DESC LIMIT ? OFFSET ?");
        assert_eq!(count, "SELECT count(*) FROM \"search\" WHERE (status = ?)");
        assert!(PageRequest::from_json(&json!({"table": "x; drop"})).is_err());
        assert!(PageRequest::from_json(&json!({"table": "t", "orderBy": "n desc"})).is_err());
        assert!(PageRequest::from_json(&json!({"table": "t", "where": "1=1; delete from t"})).is_err());
        assert!(PageRequest::from_json(&json!({"table": "t", "where": "1=1 -- x"})).is_err());
    }

    #[test]
    fn loaded_columns_are_named_and_typed() {
        assert_eq!(column_names(&["n".into(), "x rate".into(), "1st".into(), "x_rate".into()]), ["n", "x_rate", "c_1st", "x_rate_2"]);
        let sample = vec![vec![json!("1"), json!("a"), json!(null)], vec![json!(2.5), json!("3"), json!("")]];
        assert_eq!(column_types(3, &sample), ["NUMERIC", "TEXT", "TEXT"]);
    }
}
