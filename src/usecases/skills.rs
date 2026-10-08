//! The AI skills Wardian ships (ADR-2610080928): what `wardian skills` writes into a project's
//! `.claude/skills/`. Each skill is its files from skills/ plus `references/`, the docs pages it
//! needs, taken from the copies built in, so a skill works offline and matches this Wardian.

use crate::ports::assets::Assets;

/// The docs pages each skill carries as `references/<page>.md`.
const REFERENCES: &[(&str, &[&str])] = &[
    ("wardian-app-factory", &["spec", "ctx", "capabilities", "suites", "pages", "components", "data", "channels", "ai", "testing", "examples"]),
    ("wardian-app-doctor", &["spec", "ctx", "capabilities", "suites", "pages", "data", "channels", "ai", "security", "troubleshooting"]),
];

/// Put at the top of every reference, so nobody edits a copy that the next install replaces.
const REFERENCE_NOTE: &str = "<!-- A copy of a Wardian docs page, written by `wardian skills`. Do not edit it here:\n     `wardian skills --force` replaces it. Links that start with /docs/ are pages of a running Wardian. -->\n\n";

/// The skills' names, from their files.
pub fn names(assets: &dyn Assets) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = assets.skills().iter().filter_map(|(p, _)| p.split('/').next()).collect();
    names.dedup();
    names
}

/// Every file to install, as (path under `.claude/skills/`, text). Panics if a page in
/// REFERENCES is not a docs page: the pages are built in, so that is a mistake in this build.
pub fn files(assets: &dyn Assets) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = assets.skills().iter().map(|(p, body)| (p.to_string(), body.to_string())).collect();
    for (skill, pages) in REFERENCES {
        for name in *pages {
            let page = assets.docs().iter().find(|p| p.name == *name).unwrap_or_else(|| panic!("{skill} carries references/{name}.md, but there is no docs page {name}"));
            out.push((format!("{skill}/references/{name}.md"), format!("{REFERENCE_NOTE}{}", page.md)));
        }
    }
    out
}
