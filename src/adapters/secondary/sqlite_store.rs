//! Each package's database, in SQLite (ADR-2610071219): `<data dir>/db/<package>.sqlite`, written
//! private. Apps send their own SQL, so every connection carries an authorizer that keeps it inside
//! its own file: no ATTACH or DETACH, no loading extensions, and only read-only pragmas. Each
//! database is capped in size, and each statement is stopped after a time limit.

use crate::ports::db::{check_params, check_statement, quoted, safe_segment, valid_ident, Database, PageRequest, MAX_DB_BYTES, MAX_ROWS_PER_CALL, MAX_STATEMENT_MS};
use rusqlite::{
    hooks::{AuthAction, AuthContext, Authorization},
    params_from_iter,
    types::{Value as Sql, ValueRef},
    Connection, ErrorCode, OpenFlags,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Pragmas an app may use to look at its tables; their argument names a table or an index.
const INFO_PRAGMAS: &[&str] = &["table_info", "table_xinfo", "table_list", "index_list", "index_info", "index_xinfo", "foreign_key_list"];
/// Settings an app may read but not set. Every other pragma is refused.
const READ_ONLY_PRAGMAS: &[&str] = &["user_version", "page_count", "page_size"];

pub struct SqliteStore {
    dir: PathBuf,
    max_bytes: u64,
    timeout: Duration,
    open: Mutex<HashMap<String, Arc<Mutex<Connection>>>>,
}

/// What an app's own connection may do: anything inside its file except the escapes.
fn own_rules(ctx: AuthContext<'_>) -> Authorization {
    match ctx.action {
        AuthAction::Attach { .. } | AuthAction::Detach { .. } | AuthAction::CreateVtable { .. } | AuthAction::DropVtable { .. } => Authorization::Deny,
        AuthAction::Pragma { pragma_name, pragma_value } => read_pragma(pragma_name, pragma_value),
        AuthAction::Function { function_name } if function_name.eq_ignore_ascii_case("load_extension") => Authorization::Deny,
        _ => Authorization::Allow,
    }
}

/// What a page, or another package, may do: read.
fn read_rules(ctx: AuthContext<'_>) -> Authorization {
    match ctx.action {
        AuthAction::Select | AuthAction::Read { .. } | AuthAction::Recursive => Authorization::Allow,
        AuthAction::Function { function_name } if !function_name.eq_ignore_ascii_case("load_extension") => Authorization::Allow,
        AuthAction::Pragma { pragma_name, pragma_value } => read_pragma(pragma_name, pragma_value),
        _ => Authorization::Deny,
    }
}

fn read_pragma(name: &str, value: Option<&str>) -> Authorization {
    let is = |list: &[&str]| list.iter().any(|p| p.eq_ignore_ascii_case(name));
    if is(INFO_PRAGMAS) || (value.is_none() && is(READ_ONLY_PRAGMAS)) {
        Authorization::Allow
    } else {
        Authorization::Deny
    }
}

fn to_sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n.as_i64().map(Sql::Integer).unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    }
}

fn to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::String(format!("<{} bytes>", b.len())),
    }
}

impl SqliteStore {
    pub fn new(data_dir: &Path) -> SqliteStore {
        SqliteStore::with_limits(data_dir, MAX_DB_BYTES, Duration::from_millis(MAX_STATEMENT_MS))
    }

    fn with_limits(data_dir: &Path, max_bytes: u64, timeout: Duration) -> SqliteStore {
        SqliteStore { dir: data_dir.join("db"), max_bytes, timeout, open: Mutex::new(HashMap::new()) }
    }

    fn path(&self, package: &str) -> Result<PathBuf, String> {
        if !safe_segment(package) {
            return Err(format!("\"{package}\" is not an app name"));
        }
        Ok(self.dir.join(format!("{package}.sqlite")))
    }

    /// The package's own connection: created private on first use, with its limits and rules.
    fn conn(&self, package: &str) -> Result<Arc<Mutex<Connection>>, String> {
        let mut open = self.open.lock().unwrap();
        if let Some(c) = open.get(package) {
            return Ok(Arc::clone(c));
        }
        let path = self.path(package)?;
        private_file(&self.dir, &path)?;
        let conn = Connection::open(&path).map_err(|e| format!("opening {package}'s database: {e}"))?;
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0)).map_err(|e| e.to_string())?;
        let pages = i64::try_from(self.max_bytes).unwrap_or(i64::MAX) / page_size.max(512);
        conn.pragma_update(None, "max_page_count", pages).map_err(|e| e.to_string())?;
        conn.busy_timeout(Duration::from_secs(5)).map_err(|e| e.to_string())?;
        conn.authorizer(Some(own_rules)).map_err(|e| e.to_string())?;
        let conn = Arc::new(Mutex::new(conn));
        open.insert(package.to_string(), Arc::clone(&conn));
        Ok(conn)
    }

    /// Runs `f` with the statement clock started; a statement past the limit is stopped.
    fn timed<T>(&self, conn: &Connection, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
        let start = Instant::now();
        let limit = self.timeout;
        conn.progress_handler(1000, Some(move || start.elapsed() > limit)).map_err(|e| e.to_string())?;
        let out = f(conn);
        let _ = conn.progress_handler(0, None::<fn() -> bool>);
        out.map_err(|e| self.said(e))
    }

    fn said(&self, e: rusqlite::Error) -> String {
        match e.sqlite_error_code() {
            Some(ErrorCode::OperationInterrupted) => format!("the statement ran longer than {} seconds and was stopped", self.timeout.as_secs_f32()),
            Some(ErrorCode::DiskFull) => format!("this app's database is full (it may hold {} MB)", self.max_bytes / (1024 * 1024)),
            Some(ErrorCode::AuthorizationForStatementDenied) => {
                "that is not allowed in an app's database: attaching files, loading extensions, changing settings, and writing where only reading is allowed are refused".into()
            }
            _ => e.to_string(),
        }
    }

    fn page_on(&self, conn: &Connection, req: &PageRequest) -> Result<Value, String> {
        let (rows_sql, count_sql) = req.sql();
        let params: Vec<Sql> = req.params.iter().map(to_sql).collect();
        self.timed(conn, |c| {
            let total: i64 = c.query_row(&count_sql, params_from_iter(params.iter()), |r| r.get(0))?;
            let mut stmt = c.prepare(&rows_sql)?;
            let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let mut all = params.clone();
            all.push(Sql::Integer(i64::try_from(req.limit).unwrap_or(i64::MAX)));
            all.push(Sql::Integer(i64::try_from(req.offset).unwrap_or(i64::MAX)));
            let n = columns.len();
            let rows: Vec<Value> = stmt
                .query_map(params_from_iter(all.iter()), |r| Ok(Value::Array((0..n).map(|i| r.get_ref(i).map(to_json).unwrap_or(Value::Null)).collect())))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(json!({ "columns": columns, "rows": rows, "total": total, "offset": req.offset }))
        })
    }
}

/// Creates the folder and the file readable only by this user, before SQLite opens it; SQLite
/// gives its journal the same permissions.
fn private_file(dir: &Path, path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    if !path.exists() {
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        o.open(path).map_err(|e| format!("creating {}: {e}", path.display()))?;
    }
    Ok(())
}

impl Database for SqliteStore {
    fn backup(&self, package: &str, dest: &Path) -> Result<bool, String> {
        if !self.path(package)?.exists() {
            return Ok(false);
        }
        let conn = self.conn(package)?;
        let conn = conn.lock().unwrap();
        conn.backup(rusqlite::MAIN_DB, dest, None).map_err(|e| format!("copying {package}'s database: {e}"))?;
        Ok(true)
    }

    fn restore(&self, package: &str, src: &Path) -> Result<(), String> {
        let conn = self.conn(package)?;
        let mut conn = conn.lock().unwrap();
        conn.restore(rusqlite::MAIN_DB, src, None::<fn(rusqlite::backup::Progress)>).map_err(|e| format!("installing {package}'s tables: {e}"))
    }

    fn query(&self, package: &str, sql: &str, params: &[Value]) -> Result<Value, String> {
        let sql = check_statement(sql)?;
        let params: Vec<Sql> = check_params(&Value::Array(params.to_vec()))?.iter().map(to_sql).collect();
        let conn = self.conn(package)?;
        let conn = conn.lock().unwrap();
        self.timed(&conn, |c| {
            let mut stmt = c.prepare(&sql)?;
            let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            if columns.is_empty() {
                let changed = stmt.execute(params_from_iter(params.iter()))?;
                return Ok(json!({ "columns": [], "rows": [], "changed": changed, "truncated": false }));
            }
            let n = columns.len();
            let mut rows = Vec::new();
            let mut it = stmt.query(params_from_iter(params.iter()))?;
            let mut truncated = false;
            while let Some(r) = it.next()? {
                if rows.len() == MAX_ROWS_PER_CALL {
                    truncated = true;
                    break;
                }
                rows.push(Value::Array((0..n).map(|i| r.get_ref(i).map(to_json).unwrap_or(Value::Null)).collect()));
            }
            Ok(json!({ "columns": columns, "rows": rows, "changed": 0, "truncated": truncated }))
        })
    }

    fn page(&self, package: &str, req: &PageRequest, read_only: bool) -> Result<Value, String> {
        if read_only {
            // Another package's file, opened read-only for this one call.
            let path = self.path(package)?;
            if !path.exists() {
                return Err(format!("{package} has no tables yet"));
            }
            let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| format!("opening {package}'s database: {e}"))?;
            conn.authorizer(Some(read_rules)).map_err(|e| e.to_string())?;
            return self.page_on(&conn, req);
        }
        let conn = self.conn(package)?;
        let conn = conn.lock().unwrap();
        // A page only reads, even in the package's own database.
        conn.authorizer(Some(read_rules)).map_err(|e| e.to_string())?;
        let out = self.page_on(&conn, req);
        conn.authorizer(Some(own_rules)).map_err(|e| e.to_string())?;
        out
    }

    fn create_table(&self, package: &str, table: &str, columns: &[String], types: &[&str], replace: bool) -> Result<(), String> {
        if !valid_ident(table) || columns.is_empty() || !columns.iter().all(|c| valid_ident(c)) {
            return Err("table and column names must be letters, digits and _".into());
        }
        let cols: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let t = types.get(i).copied().filter(|t| matches!(*t, "TEXT" | "NUMERIC" | "INTEGER" | "REAL")).unwrap_or("TEXT");
                format!("{} {t}", quoted(c))
            })
            .collect();
        let conn = self.conn(package)?;
        let conn = conn.lock().unwrap();
        self.timed(&conn, |c| {
            if replace {
                c.execute(&format!("DROP TABLE IF EXISTS {}", quoted(table)), [])?;
            }
            c.execute(&format!("CREATE TABLE IF NOT EXISTS {} ({})", quoted(table), cols.join(", ")), [])?;
            Ok(())
        })
    }

    fn insert_rows(&self, package: &str, table: &str, columns: &[String], rows: &[Vec<Value>]) -> Result<usize, String> {
        if !valid_ident(table) || columns.is_empty() || !columns.iter().all(|c| valid_ident(c)) {
            return Err("table and column names must be letters, digits and _".into());
        }
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quoted(table),
            columns.iter().map(|c| quoted(c)).collect::<Vec<_>>().join(", "),
            vec!["?"; columns.len()].join(", ")
        );
        let conn = self.conn(package)?;
        let mut conn = conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| self.said(e))?;
        {
            let mut stmt = tx.prepare(&sql).map_err(|e| self.said(e))?;
            for r in rows {
                let vals: Vec<Sql> = (0..columns.len()).map(|i| r.get(i).map(to_sql).unwrap_or(Sql::Null)).collect();
                stmt.execute(params_from_iter(vals.iter())).map_err(|e| self.said(e))?;
            }
        }
        tx.commit().map_err(|e| self.said(e))?;
        Ok(rows.len())
    }

    fn tables(&self, package: &str) -> Result<Value, String> {
        // Asking which tables an app has must not create its database.
        if !self.path(package)?.exists() {
            return Ok(json!([]));
        }
        let conn = self.conn(package)?;
        let conn = conn.lock().unwrap();
        self.timed(&conn, |c| {
            let names: Vec<String> = c
                .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut out = Vec::new();
            for n in names {
                let cols: Vec<String> = c.prepare(&format!("PRAGMA table_info({})", quoted(&n)))?.query_map([], |r| r.get(1))?.collect::<rusqlite::Result<_>>()?;
                let rows: i64 = c.query_row(&format!("SELECT count(*) FROM {}", quoted(&n)), [], |r| r.get(0))?;
                out.push(json!({ "name": n, "columns": cols, "rows": rows }));
            }
            Ok(Value::Array(out))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str, max_bytes: u64, timeout_ms: u64) -> (SqliteStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("wardian-db-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (SqliteStore::with_limits(&dir, max_bytes, Duration::from_millis(timeout_ms)), dir)
    }

    fn refused(r: Result<Value, String>) -> bool {
        r.is_err_and(|e| e.contains("not allowed") || e.contains("not authorized") || e.contains("no such function"))
    }

    #[test]
    fn the_escapes_are_refused() {
        let (db, dir) = store("escapes", MAX_DB_BYTES, 2000);
        let other = dir.join("other.sqlite");
        assert!(refused(db.query("app", &format!("ATTACH DATABASE '{}' AS o", other.display()), &[])), "ATTACH");
        assert!(refused(db.query("app", "SELECT load_extension('x')", &[])), "load_extension");
        assert!(refused(db.query("app", "PRAGMA writable_schema = 1", &[])), "a pragma that writes");
        assert!(refused(db.query("app", "PRAGMA journal_mode = OFF", &[])), "journal_mode");
        assert!(refused(db.query("app", "PRAGMA user_version = 7", &[])), "setting a readable pragma");
        assert!(db.query("app", "PRAGMA user_version", &[]).is_ok(), "reading it");
        db.query("app", "CREATE TABLE t (a, b)", &[]).unwrap();
        assert!(db.query("app", "PRAGMA table_info(t)", &[]).is_ok(), "a read-only pragma is allowed");
        assert!(db.query("app", "SELECT 1; SELECT 2", &[]).is_err(), "one statement at a time");
        assert!(db.query("../evil", "SELECT 1", &[]).is_err(), "names stay in the folder");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(dir.join("db/app.sqlite")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn size_and_time_are_limited() {
        let (db, dir) = store("limits", 64 * 1024, 300);
        db.query("app", "CREATE TABLE t (x)", &[]).unwrap();
        let big = "x".repeat(200_000);
        let full = db.query("app", "INSERT INTO t VALUES (?)", &[json!(big)]);
        assert!(full.is_err_and(|e| e.contains("full")), "the size limit holds");
        let start = Instant::now();
        let slow = db.query("app", "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c", &[]);
        assert!(slow.is_err_and(|e| e.contains("stopped")), "a runaway statement is stopped");
        assert!(start.elapsed() < Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pages_sort_filter_and_only_read() {
        let (db, dir) = store("pages", MAX_DB_BYTES, 2000);
        let cols = vec!["n".to_string(), "host".to_string()];
        db.create_table("app", "t", &cols, &["NUMERIC", "TEXT"], true).unwrap();
        let rows: Vec<Vec<Value>> = (1..=2500).map(|i| vec![json!(i.to_string()), json!(if i % 2 == 0 { "even" } else { "odd" })]).collect();
        assert_eq!(db.insert_rows("app", "t", &cols, &rows).unwrap(), 2500);
        let req = PageRequest::from_json(&json!({"table": "t", "offset": 10, "limit": 5, "orderBy": "n", "desc": true, "where": "host = ?", "params": ["even"]})).unwrap();
        let p = db.page("app", &req, false).unwrap();
        assert_eq!(p["total"], 1250);
        assert_eq!(p["rows"][0][0], json!(2480), "numbers sort as numbers, newest first, from offset 10");
        let q = db.query("app", "SELECT n FROM t", &[]).unwrap();
        assert_eq!(q["rows"].as_array().unwrap().len(), MAX_ROWS_PER_CALL);
        assert_eq!(q["truncated"], true);
        // Another package reads read-only, and a page cannot write even through a condition.
        let other = db.page("app", &PageRequest::from_json(&json!({"table": "t", "limit": 3})).unwrap(), true).unwrap();
        assert_eq!(other["rows"].as_array().unwrap().len(), 3);
        assert!(db.page("nobody", &PageRequest::from_json(&json!({"table": "t"})).unwrap(), true).is_err());
        let sneaky = PageRequest::from_json(&json!({"table": "t", "where": "n IN (SELECT n FROM t) AND load_extension('x') IS NULL"})).unwrap();
        assert!(db.page("app", &sneaky, true).is_err());
        let tables = db.tables("app").unwrap();
        assert_eq!(tables[0]["name"], "t");
        assert_eq!(tables[0]["rows"], 2500);
        let _ = std::fs::remove_dir_all(dir);
    }
}
