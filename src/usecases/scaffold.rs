//! `wardian new` and `wardian add`: a starter package that already passes `wardian check`, and
//! component library files copied into a package (like shadcn: the package owns its copies).

use super::check::Checker;
use super::history::History;
use super::workspace;
use crate::domain::components::{files_for, page_tags, wire_suite, Added, UI_PAGE, UI_SUITE};
use crate::domain::history::Promoted;
use crate::domain::package::safe_segment;
use crate::ports::{assets::Assets, storage::FileSystem, tools::PackageTools};
use serde_json::Value;
use std::{path::Path, sync::Arc};

pub struct Scaffold {
    fs: Arc<dyn FileSystem>,
    assets: Arc<dyn Assets>,
    checker: Arc<Checker>,
}

impl PackageTools for Scaffold {
    fn check(&self, path: &Path) -> (bool, Vec<String>) {
        self.checker.check_path(path)
    }
    fn create(&self, kind: &str, path: &Path) -> Result<(), String> {
        Scaffold::create(self, kind, path)
    }
    fn add(&self, names: &[String], pkg: &Path, force: bool) -> Result<Added, String> {
        Scaffold::add(self, names, pkg, force)
    }
    fn promote(&self, app: &str, data_dir: &Path, source: &Path) -> Result<Promoted, String> {
        let working = data_dir.join("apps");
        let done = workspace::promote(&*self.fs, app, &working, source)?;
        History::new(Arc::clone(&self.fs), data_dir, &working).record(app, "promote", &format!("promoted to {}", source.display()));
        Ok(done)
    }
}

impl Scaffold {
    pub fn new(fs: Arc<dyn FileSystem>, assets: Arc<dyn Assets>, checker: Arc<Checker>) -> Scaffold {
        Scaffold { fs, assets, checker }
    }

    /// Writes a package of `kind` at `path`. "{{name}}" in text files becomes the package name.
    pub fn create(&self, kind: &str, path: &Path) -> Result<(), String> {
        let ui = match kind {
            "module" => &[][..],
            "page" => UI_PAGE,
            "suite" => UI_SUITE,
            _ => return Err(format!("unknown kind \"{kind}\"")),
        };
        let files = self.assets.template(kind).ok_or_else(|| format!("unknown kind \"{kind}\""))?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !safe_segment(name) {
            return Err(format!("\"{name}\" cannot be a package name: use letters, digits, '-', '_' or '.', not starting with '.'"));
        }
        if self.fs.exists(path) {
            return Err(format!("{} already exists", path.display()));
        }
        for (rel, bytes) in files {
            let out = path.join(rel);
            let data = if rel.ends_with(".wasm") { bytes.to_vec() } else { String::from_utf8_lossy(bytes).replace("{{name}}", name).into_bytes() };
            self.fs.write(&out, &data).map_err(|e| format!("writing {}: {e}", out.display()))?;
            if rel.ends_with(".sh") {
                self.fs.set_executable(&out);
            }
        }
        // The library files come from the library itself, so a new package always gets the
        // current components.
        for f in ui {
            let out = path.join("ui").join(f);
            let body = self.assets.ui_file(f).ok_or_else(|| format!("the component library has no {f}"))?;
            self.fs.write(&out, body.as_bytes()).map_err(|e| format!("writing {}: {e}", out.display()))?;
        }
        Ok(())
    }

    /// Copies the components' files into `<package>/ui/` and wires them in.
    pub fn add(&self, names: &[String], pkg: &Path, force: bool) -> Result<Added, String> {
        let files = files_for(names)?;
        let suite_json = pkg.join("suite.json");
        let is_suite = self.fs.is_file(&suite_json);
        if !is_suite {
            let app: Value = self
                .fs
                .read(&pkg.join("app.json"))
                .and_then(|b| serde_json::from_slice(&b).ok())
                .ok_or_else(|| format!("{} is not a package: it has no suite.json or app.json", pkg.display()))?;
            if app.get("page").and_then(Value::as_str).is_none() {
                return Err("this is a module app: Wardian draws its page, so it has nowhere to use components. Use a page app or a suite".into());
            }
        }

        let mut out = Added::default();
        let dir = pkg.join("ui");
        self.fs.create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
        for f in &files {
            let path = dir.join(f);
            let rel = format!("ui/{f}");
            if self.fs.exists(&path) && !force {
                out.skipped.push(rel);
                continue;
            }
            let body = self.assets.ui_file(f).ok_or_else(|| format!("the component library has no {f}"))?;
            self.fs.write(&path, body.as_bytes()).map_err(|e| format!("writing {}: {e}", path.display()))?;
            out.written.push(rel);
        }

        let rels: Vec<String> = files.iter().map(|f| format!("ui/{f}")).collect();
        if is_suite {
            let text = self.fs.read(&suite_json).map(|b| String::from_utf8_lossy(&b).into_owned()).ok_or("reading suite.json: cannot read it")?;
            let (body, wired) = wire_suite(&text, &rels)?;
            out.wired = wired;
            if let Some(body) = body {
                self.fs.write(&suite_json, body.as_bytes()).map_err(|e| format!("writing suite.json: {e}"))?;
            }
        } else {
            out.page_tags = page_tags(&rels);
        }
        Ok(out)
    }
}
