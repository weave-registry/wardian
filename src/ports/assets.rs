//! The files built into the program: the component library, the package templates, the kernel's
//! frame script, and the documents the docs pages and Claude read.

/// (path in a new package, contents)
pub type TemplateFiles = &'static [(&'static str, &'static [u8])];

pub trait Assets: Send + Sync {
    /// A component library file, like "button.css".
    fn ui_file(&self, name: &str) -> Option<&'static str>;
    /// The gallery page of the component library.
    fn gallery(&self) -> &'static str;
    /// The starter files of a package kind: module, page or suite.
    fn template(&self, kind: &str) -> Option<TemplateFiles>;
    /// The script that plays `Kernel` inside each suite frame, with the built-in components.
    fn frame_shim(&self) -> &'static str;
    fn spec_md(&self) -> &'static str;
    fn guide_md(&self) -> &'static str;
    /// A complete example suite, as (path, text), for Claude to learn from.
    fn example_suite(&self) -> &'static [(&'static str, &'static str)];
    /// app.schema.json or suite.schema.json.
    fn schema(&self, name: &str) -> Option<&'static str>;
}
