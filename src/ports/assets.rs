//! The files built into the program: the component library, the package templates, the kernel's
//! frame script, and the documents the docs pages and Claude read.

/// (path in a new package, contents)
pub type TemplateFiles = &'static [(&'static str, &'static [u8])];

/// One page of the documentation site, served at `/docs/<name>` ("index" is `/docs`).
pub struct DocPage {
    /// The heading it is listed under in the site's navigation, e.g. "Build apps". Empty for a
    /// page under another, named "<parent>/<page>", which its parent links to instead.
    pub group: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    /// Its Markdown file, as a path in the repository, for the "edit this page" link.
    pub source: &'static str,
    pub md: &'static str,
}

pub trait Assets: Send + Sync {
    /// A component library file, like "button.css".
    fn ui_file(&self, name: &str) -> Option<&'static str>;
    /// Every component library file name, in library order.
    fn ui_names(&self) -> Vec<&'static str>;
    /// The gallery page of the component library.
    fn gallery(&self) -> &'static str;
    /// The starter files of a package kind: module, page or suite.
    fn template(&self, kind: &str) -> Option<TemplateFiles>;
    /// The script that plays `Kernel` inside each suite frame, with the built-in components.
    fn frame_shim(&self) -> &'static str;
    fn spec_md(&self) -> &'static str;
    /// The documentation site's pages, in navigation order.
    fn docs(&self) -> &'static [DocPage];
    /// The AI skills Wardian ships (ADR-2610080928), as (path under skills/, text):
    /// "wardian-app-factory/SKILL.md", "wardian-app-factory/scripts/smoke.js" …
    fn skills(&self) -> &'static [(&'static str, &'static str)];
    /// A complete example suite, as (path, text), for Claude to learn from.
    fn example_suite(&self) -> &'static [(&'static str, &'static str)];
    /// app.schema.json or suite.schema.json.
    fn schema(&self, name: &str) -> Option<&'static str>;
}
