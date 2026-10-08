//! The assets port: the files built into the program, so Wardian is one binary that needs no
//! other files to run, check, or create packages.

use crate::ports::assets::{Assets, TemplateFiles};

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
const GUIDE_MD: &str = include_str!("../../../GUIDE.md");
const APP_SCHEMA: &str = include_str!("../../../schemas/app.schema.json");
const SUITE_SCHEMA: &str = include_str!("../../../schemas/suite.schema.json");

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
    fn guide_md(&self) -> &'static str {
        GUIDE_MD
    }
    fn example_suite(&self) -> &'static [(&'static str, &'static str)] {
        EXAMPLES
    }
    fn schema(&self, name: &str) -> Option<&'static str> {
        match name {
            "app.schema.json" => Some(APP_SCHEMA),
            "suite.schema.json" => Some(SUITE_SCHEMA),
            _ => None,
        }
    }
}
