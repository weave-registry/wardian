//! The Wardian component library as data: which components there are, which files each needs,
//! how a package lists them, and how to use them. Like shadcn, the components are copied into a
//! package rather than loaded from Wardian, so the package stays complete on its own. The files'
//! contents are built into the program (the embedded assets); this module only names them.

use serde_json::Value;

/// What `wardian add` did, for its message and for tests.
#[derive(Debug, Default, PartialEq)]
pub struct Added {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
    /// Lines added to suite.json, or the tags a page app must add by hand.
    pub wired: Vec<String>,
    pub page_tags: Vec<String>,
}

/// (name, what it is, its files).
pub const COMPONENTS: &[(&str, &str, &[&str])] = &[
    ("button", "buttons in five variants and three sizes", &["button.css"]),
    ("field", "text inputs, selects, text areas, labels and hints", &["field.css"]),
    ("card", "a bordered box with a title, a description, content and a footer", &["card.css"]),
    ("badge", "a small label for a state or a count", &["badge.css"]),
    ("table", "a data table with a sticky header and right-aligned numbers", &["table.css"]),
    ("switch", "an on/off switch made from a checkbox", &["switch.css"]),
    ("tabs", "tabs with arrow-key movement between them", &["tabs.css", "tabs.js"]),
    ("dialog", "a modal dialog, and WardianUI.confirm()", &["dialog.css", "dialog.js"]),
    ("toast", "short messages in the corner: WardianUI.toast()", &["toast.css", "toast.js"]),
    ("tooltip", "a hint on hover and keyboard focus", &["tooltip.css", "tooltip.js"]),
    ("progress", "the <wardian-progress> bar (Wardian also provides it built in)", &["progress.js"]),
    ("arrange", "Arrange: each viewer may reorder, move and hide the panels marked data-panel", &["arrange.js"]),
    ("type", "a light display serif for headlines, ledes and large figures", &["type.css"]),
    ("surface", "frosted glass panes over a slow greenhouse light", &["surface.css"]),
    ("bento", "tiles of different widths that follow the grid's own width", &["bento.css"]),
    ("receipt", "one shared message, shown the same in the sending and receiving app: WardianUI.receipt()", &["receipt.css", "receipt.js"]),
];

/// The files the named components need, theme.css first.
pub fn files_for(names: &[String]) -> Result<Vec<&'static str>, String> {
    if names.is_empty() {
        return Err("name at least one component".into());
    }
    let mut files: Vec<&str> = vec!["theme.css"];
    for n in names {
        let (_, _, fs_) = COMPONENTS
            .iter()
            .find(|(c, _, _)| c == n)
            .ok_or_else(|| format!("unknown component \"{n}\" (see wardian add --list)"))?;
        for f in *fs_ {
            if !files.contains(f) {
                files.push(f);
            }
        }
    }
    Ok(files)
}

/// Lists `rels` (ui/… files) in suite.json's styles and scripts, before the package's own files so
/// the package's styles win. Returns the new text, or None when nothing changed, and what was added.
pub fn wire_suite(text: &str, rels: &[String]) -> Result<(Option<String>, Vec<String>), String> {
    let mut s: Value = serde_json::from_str(text).map_err(|e| format!("suite.json: {e}"))?;
    let obj = s.as_object_mut().ok_or("suite.json is not an object")?;
    let mut wired = Vec::new();
    for (key, ext) in [("styles", ".css"), ("scripts", ".js")] {
        let list = obj.entry(key).or_insert_with(|| Value::Array(vec![]));
        let arr = list.as_array_mut().ok_or_else(|| format!("suite.json: {key} is not a list"))?;
        let mut at = arr.iter().rposition(|v| v.as_str().is_some_and(|s| s.starts_with("ui/"))).map_or(0, |i| i + 1);
        for r in rels.iter().filter(|r| r.ends_with(ext)) {
            if !arr.iter().any(|v| v.as_str() == Some(r)) {
                arr.insert(at, Value::String(r.clone()));
                at += 1;
                wired.push(format!("{key}: {r}"));
            }
        }
    }
    if wired.is_empty() {
        return Ok((None, wired));
    }
    Ok((Some(serde_json::to_string_pretty(&s).map_err(|e| e.to_string())? + "\n"), wired))
}

/// The tags a page app puts in its <head> for `rels`.
pub fn page_tags(rels: &[String]) -> Vec<String> {
    rels.iter()
        .map(|r| if r.ends_with(".css") { format!("<link rel=\"stylesheet\" href=\"{r}\">") } else { format!("<script src=\"{r}\"></script>") })
        .collect()
}

/// How to use each component, for Claude when it builds an app (Make an app).
pub const GUIDE: &str = r#"Classes and helpers of the Wardian component library. Colours come from theme.css tokens: var(--w-bg), --w-fg, --w-muted, --w-muted-bg, --w-border, --w-primary, --w-primary-fg, --w-destructive, --w-success, --w-radius, --w-space-1..6, --w-font, --w-font-mono. Use them for your own styles too; never hard-code a palette.
- button: <button class="w-button">Save</button>; data-variant="secondary|outline|ghost|destructive"; data-size="sm|lg|icon".
- field: <div class="w-field"><label class="w-label" for="x">Name</label><input class="w-input" id="x"><p class="w-hint">Help</p></div>; .w-input also fits <select> and <textarea>; aria-invalid="true" shows an error.
- card: <section class="w-card"><div class="w-card-header"><h2 class="w-card-title">Title</h2><p class="w-card-description">…</p></div><div class="w-card-content">…</div><div class="w-card-footer">…</div></section>
- badge: <span class="w-badge">New</span>; data-variant="secondary|outline|destructive|success".
- table: <table class="w-table"> with <thead>/<tbody>; class "num" on number cells right-aligns them.
- switch: <input type="checkbox" class="w-switch" role="switch">
- tabs: <div class="w-tabs"><div role="tablist"><button role="tab" aria-controls="p1">One</button>…</div><div role="tabpanel" id="p1">…</div>…</div> (tabs.js wires keys and ARIA).
- dialog: <dialog class="w-dialog">…</dialog> with showModal(); await WardianUI.confirm({title, text, confirm: 'Delete', destructive: true}).
- toast: WardianUI.toast('Saved', {variant: 'success'|'destructive'}).
- tooltip: data-tooltip="Hint" on any element.
- progress: <wardian-progress label="Loading"></wardian-progress>; .start(label), .update({value, max}), .done(text), .fail(text).
- arrange (page apps): mark each part <section data-panel="name" data-panel-label="Name">; put panels in containers data-arrange-column="side" / "main" inside a data-arrange-grid element, or in one parent; viewers can then reorder, move and hide them. A suite gets Arrange from Wardian without this."#;


/// The component library files each kind starts with, in `ui/`. They come from the library itself
/// (the embedded assets), so a new package always gets the current components; the copies under templates/
/// keep the templates checkable on their own, and a test keeps them identical.
pub const UI_PAGE: &[&str] = &["theme.css", "button.css", "field.css", "card.css", "arrange.js"];
pub const UI_SUITE: &[&str] = &["theme.css", "button.css", "field.css", "card.css"];

pub const KINDS: &[(&str, &str)] = &[
    ("module", "WebAssembly functions of numbers; Wardian builds the interface"),
    ("page", "WebAssembly plus your own page; for text, arrays, JSON or a real interface"),
    ("suite", "several sealed apps on one screen, talking through the kernel"),
];
