//! The driving port of the command line: check, create and extend packages.

pub use crate::domain::components::{Added, COMPONENTS, KINDS};
pub use crate::domain::package::FORMAT;
use std::path::Path;

pub trait PackageTools {
    /// Checks a package folder, a folder of packages, or a .zip/.wardian file: whether every
    /// package passed, and one block of report text per package.
    fn check(&self, path: &Path) -> (bool, Vec<String>);
    /// Writes a starter package of `kind` (module, page or suite) at `path`.
    fn create(&self, kind: &str, path: &Path) -> Result<(), String>;
    /// Copies component library files into a package, `force` replacing copies already there.
    fn add(&self, names: &[String], pkg: &Path, force: bool) -> Result<Added, String>;
}
