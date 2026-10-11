//! The embedded-assets adapter includes the example apps from a file Wardian's build.rs writes.
//! The workshop never reads them, so this build writes an empty list in its place.
use std::{env, fs, path::Path};

fn main() {
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("example_apps.rs");
    fs::write(out, "pub const EXAMPLE_APPS: &[(&str, &[u8])] = &[];\n").unwrap();
}
