//! The files port on this machine's disk.

use crate::ports::storage::FileSystem;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct LocalDisk;

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, u64)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}{name}");
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            walk(&e.path(), &format!("{rel}/"), out);
        } else {
            out.push((rel, meta.len()));
        }
    }
}

impl FileSystem for LocalDisk {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        fs::read(path).ok()
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(path, bytes).map_err(|e| e.to_string())
    }

    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("tmp");
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            let mut f = opts.open(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            fs::rename(&tmp, path)
        };
        write().map_err(|e| e.to_string())
    }

    fn remove_file(&self, path: &Path) {
        let _ = fs::remove_file(path);
    }

    fn remove_dir_all(&self, path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), String> {
        fs::rename(from, to).map_err(|e| e.to_string())
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path).map_err(|e| e.to_string())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn is_real_dir(&self, path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
    }

    fn list_dir(&self, path: &Path) -> Vec<String> {
        fs::read_dir(path).map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).collect()).unwrap_or_default()
    }

    fn walk(&self, dir: &Path) -> Vec<(String, u64)> {
        let mut out = Vec::new();
        walk(dir, "", &mut out);
        out
    }

    fn set_executable(&self, path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    fn temp_path(&self, prefix: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    fn real_name(&self, path: &Path) -> Option<String> {
        path.canonicalize().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
    }
}
