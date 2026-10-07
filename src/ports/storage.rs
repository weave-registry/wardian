//! Files on this machine: the apps folder, the data folder, and any package path the command
//! line names. Use cases decide what to read and write; the adapter does it.

use std::path::{Path, PathBuf};

/// Errors are the operating system's own words; the caller says what it was doing.
pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
    /// Writes the file, creating its folder first.
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String>;
    /// Writes through a temp file and a rename, so a crash never leaves half a file, and makes
    /// the file readable by its owner only. For keys and settings.
    fn write_private(&self, path: &Path, bytes: &[u8]) -> Result<(), String>;
    fn remove_file(&self, path: &Path);
    fn remove_dir_all(&self, path: &Path);
    fn rename(&self, from: &Path, to: &Path) -> Result<(), String>;
    fn create_dir_all(&self, path: &Path) -> Result<(), String>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    /// A real folder, not a link to one.
    fn is_real_dir(&self, path: &Path) -> bool;
    /// The names in a folder (hidden ones too); empty when it cannot be read.
    fn list_dir(&self, path: &Path) -> Vec<String>;
    /// Every file under `dir` as (path relative to it, size), hidden files and build folders too.
    fn walk(&self, dir: &Path) -> Vec<(String, u64)>;
    /// Marks a script runnable (on systems that have the notion).
    fn set_executable(&self, path: &Path);
    /// A fresh folder path under the system's temporary folder (not created).
    fn temp_path(&self, prefix: &str) -> PathBuf;
    /// The folder's own name after resolving "." and links, as `wardian check .` needs.
    fn real_name(&self, path: &Path) -> Option<String>;
}
