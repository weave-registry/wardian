//! The command line: `wardian check|new|add|docs|--version|--help`. Serving apps is the composition
//! root's job; this handles every other command and says how to run the program.

use crate::ports::service::{Exports, Pages};
use crate::ports::tools::{PackageTools, COMPONENTS, FORMAT, KINDS};
use std::path::Path;
use std::sync::Arc;

const USAGE: &str = "Wardian — runs WebAssembly apps and suites in the browser

usage:
  wardian [APPS_FOLDER]          serve the apps (default: DATA_DIR/apps, filled from ./apps on the first start);
                                 in a terminal it opens your browser, unless --no-open or WARDIAN_NO_OPEN=1
  wardian promote APP [FOLDER]   copy an app from DATA_DIR/apps into FOLDER (default: ./apps), to commit it
  wardian export APP [FILE] [--with-data]
                                 write APP from DATA_DIR/apps as a .wardian file (default: APP.wardian);
                                 --with-data adds its saved data, layout and tables, never keys
  wardian new KIND PATH          create a starter package: module, page or suite
  wardian add COMPONENT... PATH  copy UI components (button, tabs, dialog…) into a package; --list shows them
  wardian check PACKAGE...       check packages (folders, .zip or .wardian files) against SPEC.md
  wardian docs FOLDER            write the documentation site as static files into FOLDER
  wardian skills [FOLDER]        install the AI skills for building apps into FOLDER/.claude/skills
                                 (default: this folder); --force replaces them, --list names them
  wardian --version

settings come from environment variables; see README.md
ADDR other than 127.0.0.1, ::1 or localhost needs ADMIN_TOKEN: without it every program on this
machine is an admin, so Wardian refuses to listen where other machines can reach it";

/// What the arguments ask for.
pub enum Command {
    /// Serve the apps in `folder` (the first argument), or in the working folder when None.
    /// `no_open`: `--no-open` was given, so a terminal start does not open the browser.
    Serve { folder: Option<String>, no_open: bool },
    /// A command that runs and exits with this code.
    Exit(i32),
}

/// Runs any command but serving. `args` are the program's arguments without its name.
/// `data_dir` is where the working folder lives, for `promote`; `exports` builds the exporter on
/// demand, since only `export` needs the app's data.
pub fn run(args: &[String], tools: &dyn PackageTools, pages: &dyn Pages, data_dir: &Path, exports: &dyn Fn() -> Arc<dyn Exports>) -> Command {
    // `--no-open` belongs to serving, before or after the folder (ADR-2610080930).
    let no_open = args.iter().any(|a| a == "--no-open");
    let serve_args: Vec<&String> = args.iter().filter(|a| *a != "--no-open").collect();
    if no_open && serve_args.len() <= 1 && serve_args.first().is_none_or(|a| !a.starts_with('-') && !COMMANDS.contains(&a.as_str())) {
        return Command::Serve { folder: serve_args.first().map(|a| a.to_string()), no_open };
    }
    match args.first().map(String::as_str) {
        Some("export") => Command::Exit(export(&args[1..], exports)),
        Some("check") => Command::Exit(check(&args[1..], tools)),
        Some("promote") => Command::Exit(promote(&args[1..], tools, data_dir)),
        Some("new") => Command::Exit(new(&args[1..], tools)),
        Some("add") => Command::Exit(add(&args[1..], tools)),
        Some("docs") => Command::Exit(docs(&args[1..], pages)),
        Some("skills") => Command::Exit(skills(&args[1..], pages)),
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
        first => Command::Serve { folder: first.map(String::from), no_open: false },
    }
}

/// The commands other than serving, which a folder to serve may not be named.
const COMMANDS: [&str; 8] = ["export", "check", "promote", "new", "add", "docs", "skills", "help"];

/// `wardian docs <folder>`: the docs site as static files, the same pages Wardian serves at /docs.
/// The website keeps them in website/ (`wardian docs website`).
fn docs(args: &[String], pages: &dyn Pages) -> i32 {
    let [out] = args else {
        eprintln!("usage: wardian docs <folder>\n\nexample: wardian docs website   (writes website/docs/, website/schemas/, website/ui/)");
        return 2;
    };
    let out = Path::new(out);
    let files = pages.site();
    for (rel, body) in &files {
        let path = out.join(rel);
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, body));
        if let Err(e) = written {
            eprintln!("wardian docs: {}: {e}", path.display());
            return 1;
        }
    }
    println!("wrote {} files of the docs site into {}", files.len(), out.display());
    0
}

/// `wardian skills [--force] [<folder>]`: the AI skills, with their references, into
/// `<folder>/.claude/skills/` (ADR-2610080928). Like `wardian add`, it keeps a file that is already
/// there and differs, unless given --force, so a user's change is never lost.
fn skills(args: &[String], pages: &dyn Pages) -> i32 {
    if args.iter().any(|a| a == "--list" || a == "-l") {
        println!("skills (installed into FOLDER/.claude/skills/, each with references/ from these docs):");
        for n in pages.skill_names() {
            println!("  {n}");
        }
        return 0;
    }
    let force = args.iter().any(|a| a == "--force" || a == "-f");
    if let Some(a) = args.iter().find(|a| a.starts_with('-') && !matches!(a.as_str(), "--force" | "-f")) {
        eprintln!("unknown option {a}\n\nusage: wardian skills [--force] [FOLDER]\n       wardian skills --list");
        return 2;
    }
    let rest: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let folder = match rest.as_slice() {
        [] => Path::new("."),
        [f] => Path::new(f.as_str()),
        _ => {
            eprintln!("usage: wardian skills [--force] [FOLDER]\n       wardian skills --list");
            return 2;
        }
    };
    let base = folder.join(".claude").join("skills");
    let (mut written, mut kept) = (0, Vec::new());
    for (rel, body) in pages.skills() {
        let path = base.join(&rel);
        // Bytes, not text: a file that is not UTF-8, or that cannot be read, is still a user's file.
        match std::fs::read(&path) {
            Ok(old) if old == body.as_bytes() => continue,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ if !force => {
                kept.push(rel);
                continue;
            }
            _ => {}
        }
        let done = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, &body));
        if let Err(e) = done {
            eprintln!("wardian skills: {}: {e}", path.display());
            return 1;
        }
        written += 1;
    }
    for k in &kept {
        println!("  kept     {}/{k} (changed here; --force replaces it)", base.display());
    }
    println!("{} skills in {}: {written} file(s) written, {} kept, the rest already current", pages.skill_names().len(), base.display(), kept.len());
    0
}

/// `wardian export <app> [<file>] [--with-data]` (ADR-2610071248).
fn export(args: &[String], exports: &dyn Fn() -> Arc<dyn Exports>) -> i32 {
    let with_data = args.iter().any(|a| a == "--with-data");
    if let Some(a) = args.iter().find(|a| a.starts_with("--") && *a != "--with-data") {
        eprintln!("unknown option {a}\n\nusage: wardian export <app> [<file>] [--with-data]");
        return 2;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One skill with one file, so the test does not depend on the real docs.
    struct OneSkill;
    impl Pages for OneSkill {
        fn page(&self, _: &str) -> Option<String> {
            None
        }
        fn site(&self) -> Vec<(String, String)> {
            Vec::new()
        }
        fn schema(&self, _: &str) -> Option<&'static str> {
            None
        }
        fn ui_file(&self, _: &str) -> Option<&'static str> {
            None
        }
        fn gallery(&self) -> &'static str {
            ""
        }
        fn skill_names(&self) -> Vec<&'static str> {
            vec!["s"]
        }
        fn skills(&self) -> Vec<(String, String)> {
            vec![("s/SKILL.md".into(), "new text\n".into())]
        }
    }

    #[test]
    fn skills_keeps_a_changed_file_that_is_not_utf8() {
        let dir = std::env::temp_dir().join(format!("wardian-cli-skills-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join(".claude/skills/s/SKILL.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let mine = b"caf\xe9\n".to_vec(); // Latin-1, not UTF-8
        std::fs::write(&file, &mine).unwrap();
        let folder = dir.display().to_string();

        assert_eq!(skills(std::slice::from_ref(&folder), &OneSkill), 0);
        assert_eq!(std::fs::read(&file).unwrap(), mine, "a plain `wardian skills` replaced the user's file");

        assert_eq!(skills(&["--force".into(), folder], &OneSkill), 0);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new text\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_misspelled_option_is_a_usage_error() {
        let dir = std::env::temp_dir().join(format!("wardian-cli-typo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(skills(&["--forse".into(), dir.display().to_string()], &OneSkill), 2);
        assert!(!dir.exists(), "`wardian skills --forse` installed the skills");

        let no_export = || -> Arc<dyn Exports> { panic!("`wardian export notes --with-date` exported the app") };
        assert_eq!(export(&["notes".into(), "--with-date".into()], &no_export), 2);
    }
}
