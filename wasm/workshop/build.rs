//! The embedded-assets adapter includes the example apps from a file Wardian's build.rs writes.
//! The workshop never reads them, so this build writes an empty list in its place. It also reads
//! Wardian's version, for the manifest of a packaged app.
use std::{env, fs, path::Path};

fn main() {
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("example_apps.rs");
    fs::write(out, "pub const EXAMPLE_APPS: &[(&str, &[u8])] = &[];\n").unwrap();
    // A .wardian file names the Wardian it was made with: Wardian's version, not this crate's.
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    let toml = fs::read_to_string("../../Cargo.toml").unwrap();
    let version = toml.lines().find_map(|l| l.strip_prefix("version = \"")).and_then(|v| v.strip_suffix('"')).expect("Wardian's version");
    println!("cargo:rustc-env=WARDIAN_VERSION={version}");
}
