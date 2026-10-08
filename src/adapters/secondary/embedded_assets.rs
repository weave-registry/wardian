//! The assets port: the files built into the program, so Wardian is one binary that needs no
//! other files to run, check, or create packages.

use crate::ports::assets::{Assets, DocPage, TemplateFiles};

macro_rules! ui {
    ($f:literal) => {
        ($f, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/ui/", $f)))
    };
}

/// The component library, by file name (static/ui/).
const FILES: &[(&str, &str)] = &[
    ui!("theme.css"),
    ui!("button.css"),
    ui!("field.css"),
    ui!("card.css"),
    ui!("badge.css"),
    ui!("table.css"),
    ui!("switch.css"),
    ui!("tabs.css"),
    ui!("tabs.js"),
    ui!("dialog.css"),
    ui!("dialog.js"),
    ui!("toast.css"),
    ui!("toast.js"),
    ui!("tooltip.css"),
    ui!("tooltip.js"),
    ui!("progress.js"),
    ui!("arrange.js"),
];

/// The script that plays `Kernel` inside each suite frame, with the default rendering for Save as web
/// page, and the standard components every app may use.
const FRAME_SHIM: &str = concat!(
    include_str!("../../../static/snapshot.js"),
    "\n",
    include_str!("../../../static/shim.js"),
    "\n",
    include_str!("../../../static/ui/progress.js")
);
const GALLERY: &str = include_str!("../../../static/ui/index.html");
const SPEC_MD: &str = include_str!("../../../SPEC.md");
const APP_SCHEMA: &str = include_str!("../../../schemas/app.schema.json");
const SUITE_SCHEMA: &str = include_str!("../../../schemas/suite.schema.json");

/// One page of the docs site: group, name, title, and its Markdown file in the repository.
macro_rules! doc {
    ($group:literal, $name:literal, $title:literal, $src:literal) => {
        DocPage { group: $group, name: $name, title: $title, source: $src, md: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $src)) }
    };
}

/// The documentation site, in navigation order. GUIDE.md, SPEC.md, CHANGELOG.md, each example's
/// README and each ADR are pages of it as they are; everything else is in docs/site/.
const DOCS: &[DocPage] = &[
    doc!("Start here", "index", "Wardian documentation", "docs/site/index.md"),
    doc!("Start here", "install", "Install and run", "docs/site/install.md"),
    doc!("Start here", "tour", "Your first ten minutes", "docs/site/tour.md"),
    doc!("Start here", "concepts", "How Wardian works", "docs/site/concepts.md"),
    doc!("Build apps", "guide", "Building Wardian apps", "GUIDE.md"),
    doc!("Build apps", "pages", "Page apps in depth", "docs/site/pages.md"),
    doc!("Build apps", "suites", "Designing a suite", "docs/site/suites.md"),
    doc!("Build apps", "components", "Components and Arrange", "docs/site/components.md"),
    doc!("Build apps", "with-claude", "Make apps with Claude", "docs/site/with-claude.md"),
    doc!("Build apps", "ai-skills", "Build with an AI assistant", "docs/site/ai-skills.md"),
    doc!("Build apps", "testing", "Check and test an app", "docs/site/testing.md"),
    doc!("Capabilities", "data", "Keeping data", "docs/site/data.md"),
    doc!("Capabilities", "channels", "Apps that talk to each other", "docs/site/channels.md"),
    doc!("Capabilities", "ai", "Claude inside your app", "docs/site/ai.md"),
    doc!("Capabilities", "splunk", "Splunk", "docs/site/splunk.md"),
    doc!("Examples", "examples", "Example apps", "docs/site/examples.md"),
    doc!("Use Wardian", "sharing", "Share, import and remove apps", "docs/site/sharing.md"),
    doc!("Use Wardian", "drive", "Serve apps from Google Drive", "docs/site/drive.md"),
    doc!("Use Wardian", "keys", "Keys and Claude settings", "docs/site/keys.md"),
    doc!("Use Wardian", "operate", "Run Wardian for others", "docs/site/operate.md"),
    doc!("Reference", "spec", "Package format", "SPEC.md"),
    doc!("Reference", "cli", "Command line", "docs/site/cli.md"),
    doc!("Reference", "ctx", "The ctx API", "docs/site/ctx.md"),
    doc!("Reference", "capabilities", "Capabilities", "docs/site/capabilities.md"),
    doc!("Reference", "config", "Settings and environment", "docs/site/config.md"),
    doc!("Reference", "security", "Security model", "docs/site/security.md"),
    doc!("Reference", "troubleshooting", "Troubleshooting", "docs/site/troubleshooting.md"),
    doc!("Project", "architecture", "How Wardian is built", "docs/site/architecture.md"),
    doc!("Project", "decisions", "Decisions", "docs/site/decisions.md"),
    doc!("Project", "contributing", "Contributing", "docs/site/contributing.md"),
    doc!("Project", "changelog", "Changelog", "CHANGELOG.md"),
    // Pages under a listed page, reached from it rather than from the navigation (group ""): each
    // example's README under Example apps, each decision under Decisions.
    doc!("", "examples/adder", "Adder", "apps/adder/README.md"),
    doc!("", "examples/csv-explorer", "CSV explorer", "apps/csv-explorer/README.md"),
    doc!("", "examples/focus-log", "Focus log", "apps/focus-log/README.md"),
    doc!("", "examples/focus-timer", "Focus timer", "apps/focus-timer/README.md"),
    doc!("", "examples/habit-tracker", "Habit tracker", "apps/habit-tracker/README.md"),
    doc!("", "examples/image-lab", "Image lab", "apps/image-lab/README.md"),
    doc!("", "examples/life", "Game of Life", "apps/life/README.md"),
    doc!("", "examples/loan-planner", "Loan planner", "apps/loan-planner/README.md"),
    doc!("", "examples/mandelbrot", "Mandelbrot explorer", "apps/mandelbrot/README.md"),
    doc!("", "examples/meeting-notes", "Meeting notes", "apps/meeting-notes/README.md"),
    doc!("", "examples/monte-carlo", "Schedule risk (Monte Carlo)", "apps/monte-carlo/README.md"),
    doc!("", "examples/number-lab", "Number lab", "apps/number-lab/README.md"),
    doc!("", "examples/splunk-table", "Splunk table", "apps/splunk-table/README.md"),
    doc!("", "examples/text-tools", "Text tools", "apps/text-tools/README.md"),
    doc!("", "examples/unit-converter", "Unit converter", "apps/unit-converter/README.md"),
    doc!("", "examples/usl-lab", "USL lab as little apps", "apps/usl-lab/README.md"),
    doc!("", "decisions/ADR-2609121400", "ADR-2609121400: hexa is a scaffolding system with two gates", "docs/adrs/ADR-2609121400-hexa-is-a-scaffolding-system-with-two-gates.md"),
    doc!("", "decisions/ADR-2609211430", "ADR-2609211430: the domain imports only what it is allowed", "docs/adrs/ADR-2609211430-the-domain-imports-only-what-it-is-allowed.md"),
    doc!("", "decisions/ADR-2609211600", "ADR-2609211600: a reference is an import", "docs/adrs/ADR-2609211600-a-reference-is-an-import.md"),
    doc!("", "decisions/ADR-2610071055", "ADR-2610071055: viewer state lives on the server", "docs/adrs/ADR-2610071055-viewer-state-lives-on-the-server.md"),
    doc!("", "decisions/ADR-2610071106", "ADR-2610071106: Claude through Amazon Bedrock", "docs/adrs/ADR-2610071106-claude-through-amazon-bedrock.md"),
    doc!("", "decisions/ADR-2610071110", "ADR-2610071110: browser notices are not app faults", "docs/adrs/ADR-2610071110-browser-notices-are-not-app-faults.md"),
    doc!("", "decisions/ADR-2610071122", "ADR-2610071122: a working folder outside the source, and a history for each app", "docs/adrs/ADR-2610071122-a-working-folder-and-app-history.md"),
    doc!("", "decisions/ADR-2610071200", "ADR-2610071200: the domain reads JSON", "docs/adrs/ADR-2610071200-the-domain-reads-json.md"),
    doc!("", "decisions/ADR-2610071219", "ADR-2610071219: SQLite as the `db` capability, with paging", "docs/adrs/ADR-2610071219-sqlite-as-the-db-capability.md"),
    doc!("", "decisions/ADR-2610071248", "ADR-2610071248: export an app as a `.wardian` file", "docs/adrs/ADR-2610071248-export-an-app-as-a-wardian-file.md"),
    doc!("", "decisions/ADR-2610072033", "ADR-2610072033: what must be true before Wardian 1.0", "docs/adrs/ADR-2610072033-what-must-be-true-before-wardian-1-0.md"),
    doc!("", "decisions/ADR-2610072118", "ADR-2610072118: long calls run as background jobs", "docs/adrs/ADR-2610072118-long-calls-run-as-background-jobs.md"),
    doc!("", "decisions/ADR-2610080900", "ADR-2610080900: a suite is made of small parts", "docs/adrs/ADR-2610080900-a-suite-is-made-of-small-parts.md"),
    doc!("", "decisions/ADR-2610080903", "ADR-2610080903: a documentation site, and an example app for every capability", "docs/adrs/ADR-2610080903-a-docs-site-and-an-example-for-every-capability.md"),
    doc!("", "decisions/ADR-2610080905", "ADR-2610080905: save an app as a web page", "docs/adrs/ADR-2610080905-save-an-app-as-a-web-page.md"),
    doc!("", "decisions/ADR-2610080915", "ADR-2610080915: install with one command", "docs/adrs/ADR-2610080915-install-with-one-command.md"),
    doc!("", "decisions/ADR-2610080930", "ADR-2610080930: a friendly first run in the terminal", "docs/adrs/ADR-2610080930-a-friendly-first-run-in-the-terminal.md"),
    doc!("", "decisions/ADR-2610080928", "ADR-2610080928: Wardian ships its AI skills", "docs/adrs/ADR-2610080928-wardian-ships-its-ai-skills.md"),
    doc!("", "decisions/ADR-2610081003", "ADR-2610081003: a page app reaches only its own package", "docs/adrs/ADR-2610081003-a-page-app-reaches-only-its-own-package.md"),
    doc!("", "decisions/ADR-2610081041", "ADR-2610081041: every claim names its test", "docs/adrs/ADR-2610081041-every-claim-names-its-test.md"),
    doc!("", "decisions/ADR-2610081501", "ADR-2610081501: keys sealed at rest", "docs/adrs/ADR-2610081501-keys-sealed-at-rest.md"),
    doc!("", "decisions/ADR-2610081500", "ADR-2610081500: keys and Claude settings in one place", "docs/adrs/ADR-2610081500-keys-and-claude-settings-in-one-place.md"),
    doc!("", "decisions/ADR-2610081900", "ADR-2610081900: examples run on the website", "docs/adrs/ADR-2610081900-examples-run-on-the-website.md"),
    doc!("", "decisions/ADR-2610081700", "ADR-2610081700: back up the master key", "docs/adrs/ADR-2610081700-back-up-the-master-key.md"),
    doc!("", "decisions/ADR-2610081600", "ADR-2610081600: example apps are built in", "docs/adrs/ADR-2610081600-example-apps-are-built-in.md"),
];

/// One file of a shipped skill (skills/).
macro_rules! skill {
    ($f:literal) => {
        ($f, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/skills/", $f)))
    };
}

/// The AI skills, by path under skills/ (ADR-2610080928). Their references/ are added on install,
/// from DOCS.
const SKILLS: &[(&str, &str)] = &[
    skill!("wardian-app-factory/SKILL.md"),
    skill!("wardian-app-factory/scripts/smoke.js"),
    skill!("wardian-app-doctor/SKILL.md"),
];

/// The starter files of each package kind (templates/).
macro_rules! file {
    ($out:literal, $src:literal) => {
        ($out, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/", $src)) as &[u8])
    };
}

const MODULE: TemplateFiles = &[
    file!("app.json", "module/app.json"),
    file!("app.wasm", "module/app.wasm"),
    file!("src/lib.rs", "module/src/lib.rs"),
    file!("Cargo.toml", "module/Cargo.toml"),
    file!("build.sh", "module/build.sh"),
    file!("README.md", "module/README.md"),
    file!(".gitignore", "module/.gitignore"),
];

const PAGE: TemplateFiles = &[
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
const SUITE: TemplateFiles = &[
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

/// The example suite Claude reads before it builds one.
const EXAMPLES: &[(&str, &str)] = &[
    ("suite.json", include_str!("../../../templates/suite/suite.json")),
    ("style.css", include_str!("../../../templates/suite/style.css")),
    ("header.html", include_str!("../../../templates/suite/header.html")),
    ("apps/input/app.js", include_str!("../../../templates/suite/apps/input/app.js")),
    ("apps/input/view.html", include_str!("../../../templates/suite/apps/input/view.html")),
    ("apps/output/app.js", include_str!("../../../templates/suite/apps/output/app.js")),
    ("apps/output/view.html", include_str!("../../../templates/suite/apps/output/view.html")),
    ("apps/text/app.js", include_str!("../../../templates/suite/apps/text/app.js")),
];


pub struct Embedded;

impl Assets for Embedded {
    fn ui_file(&self, name: &str) -> Option<&'static str> {
        FILES.iter().find(|(n, _)| *n == name).map(|(_, body)| *body)
    }
    fn ui_names(&self) -> Vec<&'static str> {
        FILES.iter().map(|(n, _)| *n).collect()
    }
    fn gallery(&self) -> &'static str {
        GALLERY
    }
    fn template(&self, kind: &str) -> Option<TemplateFiles> {
        match kind {
            "module" => Some(MODULE),
            "page" => Some(PAGE),
            "suite" => Some(SUITE),
            _ => None,
        }
    }
    fn frame_shim(&self) -> &'static str {
        FRAME_SHIM
    }
    fn spec_md(&self) -> &'static str {
        SPEC_MD
    }
    fn docs(&self) -> &'static [DocPage] {
        DOCS
    }
    fn skills(&self) -> &'static [(&'static str, &'static str)] {
        SKILLS
    }
    fn example_suite(&self) -> &'static [(&'static str, &'static str)] {
        EXAMPLES
    }
    fn example_apps(&self) -> &'static [(&'static str, &'static [u8])] {
        example_apps::EXAMPLE_APPS
    }
    fn host_file(&self, name: &str) -> Option<&'static str> {
        match name {
            "kernel.html" => Some(include_str!("../../../static/kernel.html")),
            "state.js" => Some(include_str!("../../../static/state.js")),
            "channels.js" => Some(include_str!("../../../static/channels.js")),
            _ => None,
        }
    }
    fn schema(&self, name: &str) -> Option<&'static str> {
        match name {
            "app.schema.json" => Some(APP_SCHEMA),
            "suite.schema.json" => Some(SUITE_SCHEMA),
            _ => None,
        }
    }
}

/// The example apps, listed by build.rs from the folders `.gitignore` names.
mod example_apps {
    include!(concat!(env!("OUT_DIR"), "/example_apps.rs"));
}
