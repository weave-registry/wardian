//! Each package's own SQLite database (ADR-2610071219). The adapter keeps every call inside the
//! package's file and within the limits the domain sets.

pub use crate::domain::package::safe_segment;
pub use crate::domain::db::{check_params, check_statement, column_names, column_types, quoted, valid_ident, PageRequest, MAX_DB_BYTES, MAX_LOADED_ROWS, MAX_ROWS_PER_CALL, MAX_STATEMENT_MS};
use serde_json::Value;
use std::path::Path;

pub trait Database: Send + Sync {
    /// One statement in `package`'s database: `{columns, rows, changed, truncated}`, at most
    /// MAX_ROWS_PER_CALL rows.
    fn query(&self, package: &str, sql: &str, params: &[Value]) -> Result<Value, String>;
    /// One page of a table: `{columns, rows, total, offset}`. With `read_only`, the package's file
    /// is opened read-only and the statement may only read: how another package reads it.
    fn page(&self, package: &str, req: &PageRequest, read_only: bool) -> Result<Value, String>;
    /// Creates `table` with these columns and declared types (replacing it when `replace`).
    fn create_table(&self, package: &str, table: &str, columns: &[String], types: &[&str], replace: bool) -> Result<(), String>;
    /// Adds rows in one transaction; returns how many.
    fn insert_rows(&self, package: &str, table: &str, columns: &[String], rows: &[Vec<Value>]) -> Result<usize, String>;
    /// The package's tables: `[{name, columns, rows}]`.
    fn tables(&self, package: &str) -> Result<Value, String>;
    /// Copies the package's database to `dest`, consistent even while in use (SQLite's backup).
    /// False when the package has no database yet.
    fn backup(&self, package: &str, dest: &Path) -> Result<bool, String>;
    /// Replaces the package's database with the one at `src` (ADR-2610071248).
    fn restore(&self, package: &str, src: &Path) -> Result<(), String>;
}
