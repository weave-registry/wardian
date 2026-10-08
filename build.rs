//! Builds the example apps into the program, so every Wardian can fill its working folder with them,
//! however it was installed or started. The examples are the folders `.gitignore` names with
//! `!/apps/<name>/`, the same ones git tracks; build output inside them is left out.

use std::{env, fs, path::Path};

const SKIP: [&str; 3] = ["target", "node_modules", "Cargo.lock"];

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = fs::read_dir(dir).map(|r| r.flatten().collect()).unwrap_or_default();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || SKIP.contains(&name.as_str()) {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.is_file() {
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, path.canonicalize().unwrap().to_string_lossy().into_owned()));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=.gitignore");
    println!("cargo:rerun-if-changed=apps");
    let apps = Path::new("apps");
    let names: Vec<String> = fs::read_to_string(".gitignore")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("!/apps/").map(|n| n.trim_end_matches('/').to_string()))
        .filter(|n| apps.join(n).is_dir())
        .collect();
    let mut files = Vec::new();
    for n in &names {
        walk(apps, &apps.join(n), &mut files);
    }
    let mut code = String::from("/// Every file of the example apps, as (path under apps/, contents). Written by build.rs.\npub const EXAMPLE_APPS: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &files {
        code += &format!("    ({rel:?}, include_bytes!({abs:?})),\n");
    }
    code += "];\n";
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("example_apps.rs"), code).unwrap();
}
