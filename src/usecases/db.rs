//! The `db` capability (ADR-2610071219): each package's own SQLite database, through the
//! `Database` port. The web server has already checked that the app declares `db`, and, for a
//! read of another package's tables, that the user allowed it.

use crate::ports::{
    db::{check_params, column_types, valid_ident, Database, PageRequest},
    service::Tables,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub struct Db {
    db: Arc<dyn Database>,
}

impl Db {
    pub fn new(db: Arc<dyn Database>) -> Db {
        Db { db }
    }
}

impl Tables for Db {
    fn query(&self, package: &str, sql: &str, params: &Value) -> Result<Value, String> {
        self.db.query(package, sql, &check_params(params)?)
    }

    fn page(&self, package: &str, source: Option<&str>, request: &Value) -> Result<Value, String> {
        let req = PageRequest::from_json(request)?;
        match source.filter(|s| *s != package) {
            Some(other) => self.db.page(other, &req, true),
            None => self.db.page(package, &req, false),
        }
    }

    fn insert(&self, package: &str, body: &Value) -> Result<Value, String> {
        let table = body["table"].as_str().unwrap_or("");
        let columns: Vec<String> = body["columns"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default();
        if !valid_ident(table) || columns.is_empty() || !columns.iter().all(|c| valid_ident(c)) {
            return Err("give a table and its columns: letters, digits and _".into());
        }
        let rows: Vec<Vec<Value>> = body["rows"].as_array().ok_or("rows must be a list of lists")?.iter().map(|r| r.as_array().cloned().unwrap_or_default()).collect();
        if body["create"].as_bool().unwrap_or(false) || body["replace"].as_bool().unwrap_or(false) {
            let sample: Vec<Vec<Value>> = rows.iter().take(500).cloned().collect();
            self.db.create_table(package, table, &columns, &column_types(columns.len(), &sample), body["replace"].as_bool().unwrap_or(false))?;
        }
        let n = self.db.insert_rows(package, table, &columns, &rows)?;
        Ok(json!({ "inserted": n }))
    }

    fn tables(&self, package: &str) -> Result<Value, String> {
        Ok(json!({ "tables": self.db.tables(package)? }))
    }
}
