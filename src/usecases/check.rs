//! `wardian check <folder|zip>...`: tests packages against SPEC.md before they are imported,
//! and says what is wrong in words. A zip is first unpacked by the real importer into a
//! temporary folder, so the check judges exactly what an import would produce.

use super::import::import_zip;
use crate::domain::check::{check_package, format_report, Report};
use crate::domain::package::APP_MARKERS;
use crate::ports::storage::FileSystem;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Checker {
    fs: Arc<dyn FileSystem>,
}

impl Checker {
    pub fn new(fs: Arc<dyn FileSystem>) -> Checker {
        Checker { fs }
    }

    fn is_app(&self, dir: &Path) -> bool {
        APP_MARKERS.iter().any(|m| self.fs.is_file(&dir.join(m)))
    }

    fn check_app(&self, dir: &Path, name: &str, r: &mut Report) -> String {
        let files = self.fs.walk(dir);
        check_package(name, &files, &|rel| self.fs.read(&dir.join(rel)), r)
    }

    /// Checks one app folder whose name is given separately (the folder may be a
    /// staging copy). Returns whether it passed, and the report as text.
    pub fn check_dir(&self, dir: &Path, name: &str) -> (bool, String) {
        let mut r = Report::default();
        let summary = self.check_app(dir, name, &mut r);
        (r.errors.is_empty(), format_report(name, &summary, &r))
    }

    /// Checks a package folder, a folder of packages, or a .zip/.wardian file. Returns whether
    /// every package passed, and one block of text per package.
    pub fn check_path(&self, path: &Path) -> (bool, Vec<String>) {
        let shown = path.display();
        let report = |title: &str, summary: String, r: Report| (r.errors.is_empty(), format_report(title, &summary, &r));
        if self.fs.is_file(path) {
            let tmp = self.fs.temp_path("wardian-check");
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("package.zip");
            let result = self.fs.read(path).ok_or_else(|| "cannot read the file".to_string()).and_then(|bytes| import_zip(&*self.fs, &bytes, name, &tmp, true));
            let out = match result {
                Err(e) => (false, vec![format!("{shown}\n  error    {e}\n")]),
                Ok(done) => {
                    let mut all_ok = true;
                    let mut blocks = Vec::new();
                    for app in &done.apps {
                        let mut r = Report::default();
                        // What the importer left out is worth knowing, not fatal.
                        for s in &done.skipped {
                            r.warn(format!("import skipped {s}"));
                        }
                        let summary = self.check_app(&tmp.join(app), app, &mut r);
                        let (ok, text) = report(&format!("{shown} → {app}"), summary, r);
                        all_ok &= ok;
                        blocks.push(text);
                    }
                    (all_ok, blocks)
                }
            };
            self.fs.remove_dir_all(&tmp);
            return out;
        }
        if !self.fs.is_dir(path) {
            return (false, vec![format!("{shown}\n  error    not found\n")]);
        }
        if self.is_app(path) {
            let name = self.fs.real_name(path).unwrap_or_default();
            let mut r = Report::default();
            let summary = self.check_app(path, &name, &mut r);
            let (ok, text) = report(&shown.to_string(), summary, r);
            return (ok, vec![text]);
        }
        // A folder of apps, like the host's apps folder.
        let mut apps: Vec<PathBuf> = self.fs.list_dir(path).into_iter().map(|n| path.join(n)).filter(|p| self.is_app(p)).collect();
        apps.sort();
        if apps.is_empty() {
            return (false, vec![format!("{shown}\n  error    no app here: a package needs app.wasm or suite.json at its top\n")]);
        }
        let mut ok = true;
        let mut blocks = Vec::new();
        for app in apps {
            let name = app.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut r = Report::default();
            let summary = self.check_app(&app, &name, &mut r);
            let (passed, text) = report(&app.display().to_string(), summary, r);
            ok &= passed;
            blocks.push(text);
        }
        (ok, blocks)
    }
}
