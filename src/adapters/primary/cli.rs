//! The command line: `wardian check|new|add|--version|--help`. Serving apps is the composition
//! root's job; this handles every other command and says how to run the program.

use crate::ports::service::Exports;
use crate::ports::tools::{PackageTools, COMPONENTS, FORMAT, KINDS};
use std::path::Path;
use std::sync::Arc;

const USAGE: &str = "Wardian — runs WebAssembly apps and suites in the browser

usage:
  wardian [APPS_FOLDER]          serve the apps (default: DATA_DIR/apps, filled from ./apps on the first start)
  wardian promote APP [FOLDER]   copy an app from DATA_DIR/apps into FOLDER (default: ./apps), to commit it
  wardian export APP [FILE] [--with-data]
                                 write APP from DATA_DIR/apps as a .wardian file (default: APP.wardian);
                                 --with-data adds its saved data, layout and tables, never keys
  wardian new KIND PATH          create a starter package: module, page or suite
  wardian add COMPONENT... PATH  copy UI components (button, tabs, dialog…) into a package; --list shows them
  wardian check PACKAGE...       check packages (folders, .zip or .wardian files) against SPEC.md
  wardian --version

settings come from environment variables; see README.md
ADDR other than 127.0.0.1, ::1 or localhost needs ADMIN_TOKEN: without it every program on this
machine is an admin, so Wardian refuses to listen where other machines can reach it";

/// What the arguments ask for.
pub enum Command {
    /// Serve the apps in this folder (the first argument), or in the working folder when None.
    Serve(Option<String>),
    /// A command that runs and exits with this code.
    Exit(i32),
}

/// Runs any command but serving. `args` are the program's arguments without its name.
/// `data_dir` is where the working folder lives, for `promote`; `exports` builds the exporter on
/// demand, since only `export` needs the app's data.
pub fn run(args: &[String], tools: &dyn PackageTools, data_dir: &Path, exports: &dyn Fn() -> Arc<dyn Exports>) -> Command {
    match args.first().map(String::as_str) {
        Some("export") => Command::Exit(export(&args[1..], exports)),
        Some("check") => Command::Exit(check(&args[1..], tools)),
        Some("promote") => Command::Exit(promote(&args[1..], tools, data_dir)),
        Some("new") => Command::Exit(new(&args[1..], tools)),
        Some("add") => Command::Exit(add(&args[1..], tools)),
        Some("--version" | "-V") => {
            println!("Wardian {} (package format {FORMAT})", env!("CARGO_PKG_VERSION"));
            Command::Exit(0)
        }
        Some("--help" | "-h" | "help") => {
            println!("{USAGE}");
            Command::Exit(0)
        }
        Some(a) if a.starts_with('-') => {
            eprintln!("unknown option {a}\n\n{USAGE}");
            Command::Exit(2)
        }
        first => Command::Serve(first.map(String::from)),
    }
}

/// `wardian export <app> [<file>] [--with-data]` (ADR-2610071248).
fn export(args: &[String], exports: &dyn Fn() -> Arc<dyn Exports>) -> i32 {
    let with_data = args.iter().any(|a| a == "--with-data");
    let rest: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let (app, file) = match rest.as_slice() {
        [app] => (app.as_str(), format!("{app}.wardian")),
        [app, file] => (app.as_str(), file.to_string()),
        _ => {
            eprintln!("usage: wardian export <app> [<file>] [--with-data]");
            return 2;
        }
    };
    let ex = exports();
    let bytes = match ex.export(app, with_data) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("wardian export: {e}");
            return 1;
        }
    };
    if let Err(e) = std::fs::write(&file, &bytes) {
        eprintln!("wardian export: writing {file}: {e}");
        return 1;
    }
    println!("wrote {file} ({} KB){}", bytes.len().div_ceil(1024), if with_data { ", with the app's data; keys, accounts and permission answers are never included" } else { "" });
    0
}

/// `wardian promote <app> [<folder>]`: copies an app made or changed inside Wardian back into
/// the repository's apps folder, where it can be reviewed and committed (ADR-2610071122).
fn promote(args: &[String], tools: &dyn PackageTools, data_dir: &Path) -> i32 {
    let (app, folder) = match args {
        [app] => (app.as_str(), "apps"),
        [app, folder] => (app.as_str(), folder.as_str()),
        _ => {
            eprintln!("usage: wardian promote <app> [<folder>]   (folder default: ./apps)");
            return 2;
        }
    };
    match tools.promote(app, data_dir, Path::new(folder)) {
        Err(e) => {
            eprintln!("wardian promote: {e}");
            1
        }
        Ok(p) => {
            let show = |what: &str, list: &[String]| {
                for f in list {
                    println!("  {what:<8} {folder}/{app}/{f}");
                }
            };
            show("added", &p.added);
            show("changed", &p.changed);
            show("removed", &p.removed);
            if p.added.is_empty() && p.changed.is_empty() && p.removed.is_empty() {
                println!("{folder}/{app} already matches {}/apps/{app}", data_dir.display());
            } else {
                println!("
review it with: git diff -- {folder}/{app}");
            }
            0
        }
    }
}

/// Runs the checks and prints the report. Returns the process exit code:
/// 0 when no package has errors, 1 otherwise, 2 for bad usage.
fn check(paths: &[String], tools: &dyn PackageTools) -> i32 {
    if paths.is_empty() {
        eprintln!("usage: wardian check <package folder | .zip | .wardian>...");
        return 2;
    }
    let mut failed = false;
    for p in paths {
        let (ok, blocks) = tools.check(Path::new(p));
        for b in blocks {
            println!("{b}");
        }
        failed |= !ok;
    }
    i32::from(failed)
}

/// `wardian new <kind> <path>`: writes a starter package that already passes `wardian check`.
fn new(args: &[String], tools: &dyn PackageTools) -> i32 {
    let usage = || {
        eprintln!("usage: wardian new <kind> <path>\n\nkinds:");
        for (k, what) in KINDS {
            eprintln!("  {k:<8} {what}");
        }
        eprintln!("\nexample: wardian new page apps/hello");
        2
    };
    let [kind, path] = args else { return usage() };
    if !KINDS.iter().any(|(k, _)| k == kind) {
        eprintln!("unknown kind \"{kind}\"\n");
        return usage();
    }
    let path = Path::new(path);
    if let Err(e) = tools.create(kind, path) {
        eprintln!("wardian new: {e}");
        return 1;
    }
    println!("created a {kind} package in {}\n", path.display());
    let code = check(&[path.display().to_string()], tools);
    println!("next: read {}/README.md, change it, then run `wardian check {}`", path.display(), path.display());
    code
}

/// `wardian add <component...> <package>`: copies component library files into a package.
fn add(args: &[String], tools: &dyn PackageTools) -> i32 {
    let list = || {
        println!("components (copied into PACKAGE/ui/, with theme.css):");
        for (n, what, _) in COMPONENTS {
            println!("  {n:<9} {what}");
        }
        println!("\nsee them all: open /ui/ on a running Wardian");
    };
    if args.iter().any(|a| a == "--list" || a == "-l") {
        list();
        return 0;
    }
    let force = args.iter().any(|a| a == "--force" || a == "-f");
    let rest: Vec<String> = args.iter().filter(|a| !a.starts_with('-')).cloned().collect();
    let Some((pkg, names)) = rest.split_last().filter(|(_, n)| !n.is_empty()) else {
        eprintln!("usage: wardian add [--force] COMPONENT... PACKAGE\n       wardian add --list\n\nexample: wardian add button tabs toast apps/my-suite\n");
        list();
        return 2;
    };
    let pkg = Path::new(pkg);
    match tools.add(names, pkg, force) {
        Err(e) => {
            eprintln!("wardian add: {e}");
            1
        }
        Ok(a) => {
            for w in &a.written {
                println!("  wrote    {}/{w}", pkg.display());
            }
            for s in &a.skipped {
                println!("  kept     {}/{s} (already there; --force replaces it)", pkg.display());
            }
            for w in &a.wired {
                println!("  added to suite.json {w}");
            }
            if !a.page_tags.is_empty() {
                println!("\nadd these lines to your page's <head>:");
                for t in &a.page_tags {
                    println!("  {t}");
                }
            }
            println!();
            check(&[pkg.display().to_string()], tools)
        }
    }
}
