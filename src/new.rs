//! `rustle new <module|page|suite> <path>`: writes a starter package that
//! already passes `rustle check`. The templates are built into the program
//! from templates/, so it works anywhere.

use crate::source::safe_segment;
use std::{fs, path::Path};

/// (path in the new package, contents). "{{name}}" in text files becomes the
/// package name.
type Files = &'static [(&'static str, &'static [u8])];

macro_rules! file {
    ($out:literal, $src:literal) => {
        ($out, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/", $src)) as &[u8])
    };
}

const MODULE: Files = &[
    file!("app.json", "module/app.json"),
    file!("app.wasm", "module/app.wasm"),
    file!("src/lib.rs", "module/src/lib.rs"),
    file!("Cargo.toml", "module/Cargo.toml"),
    file!("build.sh", "module/build.sh"),
    file!("README.md", "module/README.md"),
    file!(".gitignore", "module/.gitignore"),
];

const PAGE: Files = &[
    file!("app.json", "page/app.json"),
    file!("index.html", "page/index.html"),
    file!("app.js", "page/app.js"),
    file!("app.wasm", "page/app.wasm"),
    file!("src/lib.rs", "page/src/lib.rs"),
    file!("Cargo.toml", "page/Cargo.toml"),
    file!("build.sh", "page/build.sh"),
    file!("README.md", "page/README.md"),
    file!(".gitignore", "page/.gitignore"),
];

// The suite reuses the page's Rust source, so the two never drift apart.
const SUITE: Files = &[
    file!("suite.json", "suite/suite.json"),
    file!("style.css", "suite/style.css"),
    file!("header.html", "suite/header.html"),
    file!("apps/text/app.js", "suite/apps/text/app.js"),
    file!("apps/input/app.js", "suite/apps/input/app.js"),
    file!("apps/input/view.html", "suite/apps/input/view.html"),
    file!("apps/output/app.js", "suite/apps/output/app.js"),
    file!("apps/output/view.html", "suite/apps/output/view.html"),
    file!("text.wasm", "suite/text.wasm"),
    file!("src/lib.rs", "page/src/lib.rs"),
    file!("Cargo.toml", "page/Cargo.toml"),
    file!("build.sh", "suite/build.sh"),
    file!("README.md", "suite/README.md"),
    file!(".gitignore", "suite/.gitignore"),
];

pub const KINDS: &[(&str, &str)] = &[
    ("module", "WebAssembly functions of numbers; rustle builds the interface"),
    ("page", "WebAssembly plus your own page; for text, arrays, JSON or a real interface"),
    ("suite", "several sealed apps on one screen, talking through the kernel"),
];

/// Writes the package. Returns the error to show, if any.
pub fn create(kind: &str, path: &Path) -> Result<(), String> {
    let files = match kind {
        "module" => MODULE,
        "page" => PAGE,
        "suite" => SUITE,
        _ => return Err(format!("unknown kind \"{kind}\"")),
    };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !safe_segment(name) {
        return Err(format!("\"{name}\" cannot be a package name: use letters, digits, '-', '_' or '.', not starting with '.'"));
    }
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    for (rel, bytes) in files {
        let out = path.join(rel);
        fs::create_dir_all(out.parent().unwrap()).map_err(|e| format!("creating {}: {e}", out.display()))?;
        let is_text = !rel.ends_with(".wasm");
        let data = if is_text {
            String::from_utf8_lossy(bytes).replace("{{name}}", name).into_bytes()
        } else {
            bytes.to_vec()
        };
        fs::write(&out, data).map_err(|e| format!("writing {}: {e}", out.display()))?;
        #[cfg(unix)]
        if rel.ends_with(".sh") {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&out, fs::Permissions::from_mode(0o755));
        }
    }
    Ok(())
}

/// The `rustle new` command. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let usage = || {
        eprintln!("usage: rustle new <kind> <path>\n\nkinds:");
        for (k, what) in KINDS {
            eprintln!("  {k:<8} {what}");
        }
        eprintln!("\nexample: rustle new page apps/hello");
        2
    };
    let [kind, path] = args else { return usage() };
    if !KINDS.iter().any(|(k, _)| k == kind) {
        eprintln!("unknown kind \"{kind}\"\n");
        return usage();
    }
    let path = Path::new(path);
    if let Err(e) = create(kind, path) {
        eprintln!("rustle new: {e}");
        return 1;
    }
    println!("created a {kind} package in {}\n", path.display());
    let code = crate::check::run(&[path.display().to_string()]);
    println!("next: read {}/README.md, change it, then run `rustle check {}`", path.display(), path.display());
    code
}

#[cfg(test)]
mod tests {
    use super::{create, KINDS};

    #[test]
    fn every_template_passes_check() {
        let base = std::env::temp_dir().join(format!("rustle-new-test-{}", std::process::id()));
        for (kind, _) in KINDS {
            let dir = base.join(format!("demo-{kind}"));
            create(kind, &dir).unwrap();
            assert!(crate::check::check_path(&dir), "{kind} template fails rustle check");
            let readme = std::fs::read_to_string(dir.join("README.md")).unwrap();
            assert!(readme.contains(&format!("demo-{kind}")) && !readme.contains("{{name}}"));
        }
        assert!(create("page", &base.join("demo-page")).is_err(), "must not overwrite");
        let _ = std::fs::remove_dir_all(base);
    }
}
