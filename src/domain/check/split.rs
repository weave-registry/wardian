//! A suite is made of small parts (ADR-2610080900): `wardian check` warns about a suite part that
//! does too much, and names the parts it could split into. Only warnings: a big part still runs,
//! but viewers cannot arrange what lives in one panel.

/// More lines than this in one part's `app.js` usually means several jobs in one part.
const MAX_PART_LINES: usize = 400;

/// What the check knows about one suite part when it judges its size.
pub struct PartSize {
    pub name: String,
    /// The part has a slot, so a panel of its own.
    pub has_view: bool,
    /// Lines in its `app.js`.
    pub lines: usize,
    /// Its `app.js`, to find the sections it marks with `// ---------- name ----------`.
    pub code: String,
    /// Its `view.html`, to find its `<h2>` headings.
    pub view: String,
}

/// The text of each `<h2>` in `html`, tags removed and spaces collapsed.
fn headings(html: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<h2").map(|i| at + i) {
        let after = &lower[i + 3..];
        // "<h2>" or "<h2 class=…>", not "<h20".
        if !after.starts_with(|c: char| c == '>' || c.is_whitespace()) {
            at = i + 3;
            continue;
        }
        let Some(open_end) = lower[i..].find('>').map(|j| i + j + 1) else { break };
        let close = lower[open_end..].find("</h2").map(|j| open_end + j).unwrap_or(lower.len());
        out.push(words(&strip_tags(&html[open_end..close])));
        at = close.max(open_end);
    }
    out.into_iter().filter(|h| !h.is_empty()).collect()
}

/// The names of the sections a script marks with `// ---------- name ----------`.
fn sections(code: &str) -> Vec<String> {
    code.lines()
        .filter_map(|l| l.trim().strip_prefix("// ----------"))
        .filter_map(|l| l.strip_suffix("----------"))
        .map(words)
        .filter(|s| !s.is_empty())
        .collect()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

fn words(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A part name for a heading or section: its first word that says something, in lowercase.
fn part_name(title: &str) -> String {
    let w: Vec<String> = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(w.to_ascii_lowercase().as_str(), "the" | "a" | "an" | "of" | "to" | "and"))
        .take(1)
        .map(|w| w.to_ascii_lowercase())
        .collect();
    w.concat()
}

fn ideas(titles: &[String]) -> String {
    let mut names: Vec<String> = Vec::new();
    for n in titles.iter().map(|t| part_name(t)).filter(|n| !n.is_empty()) {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    if names.len() < 2 {
        names = ["inputs", "results", "export"].map(String::from).to_vec();
    }
    names.join(", ")
}

/// The warnings for a suite's parts: one per part over [`MAX_PART_LINES`], and one when the suite
/// has a single panel whose view holds more than one `<h2>`.
pub fn split_warnings(parts: &[PartSize]) -> Vec<String> {
    let mut out = Vec::new();
    for p in parts.iter().filter(|p| p.lines > MAX_PART_LINES) {
        let h = headings(&p.view);
        let titles = if h.len() > 1 { h } else { sections(&p.code) };
        out.push(format!(
            "{}: app.js has {} lines; a part this big usually does several jobs. Give each job its own part, such as {} (ADR-2610080900)",
            p.name,
            p.lines,
            ideas(&titles)
        ));
    }
    let views: Vec<&PartSize> = parts.iter().filter(|p| p.has_view).collect();
    if let [only] = views.as_slice() {
        let h = headings(&only.view);
        if h.len() > 1 {
            out.push(format!(
                "{} is the only panel, and its view.html has {} headings ({}): give each its own part, such as {}, so viewers can arrange them (ADR-2610080900)",
                only.name,
                h.len(),
                h.join(", "),
                ideas(&h)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, has_view: bool, lines: usize, view: &str) -> PartSize {
        let code = "// ---------- the form ----------\n// ---------- sending to other apps ----------\n".to_string() + &"x\n".repeat(lines.saturating_sub(2));
        PartSize { name: name.into(), has_view, lines, code, view: view.into() }
    }

    #[test]
    fn check_finds_headings_and_sections() {
        assert_eq!(headings("<h2>Search</h2><p>x</p><H2 class=\"r\">The <b>Result</b>\n here</H2><h20>no</h20>"), vec!["Search", "The Result here"]);
        assert_eq!(sections("  // ---------- the form ----------\nlet a;\n// ---------- CSV ----------"), vec!["the form", "CSV"]);
        assert_eq!(part_name("Find in results"), "find");
        assert_eq!(part_name("The result"), "result");
    }

    #[test]
    fn check_warns_about_one_big_part_and_names_its_jobs() {
        let w = split_warnings(&[part("table", true, 650, "<h2>Search</h2><h2>Result</h2>")]);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].starts_with("table: app.js has 650 lines") && w[0].contains("such as search, result"), "{}", w[0]);
        assert!(w[1].contains("only panel") && w[1].contains("2 headings (Search, Result)"), "{}", w[1]);
        // Without headings, the sections of the code name the parts.
        let w = split_warnings(&[part("big", true, 401, "<p>x</p>"), part("other", true, 10, "")]);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("such as form, sending"), "{}", w[0]);
    }

    #[test]
    fn check_is_quiet_about_small_parts() {
        assert!(split_warnings(&[part("a", true, 400, "<h2>One</h2>")]).is_empty(), "400 lines is the limit, not over it");
        // Several panels, each with its own heading: the shape the ADR asks for.
        let parts = [part("search", true, 237, "<h2>Search</h2>"), part("rows", true, 144, "<h2>Result</h2><h2>More</h2>"), part("engine", false, 300, "")];
        assert!(split_warnings(&parts).is_empty());
        // One panel with one heading is a small app, not a tangle.
        assert!(split_warnings(&[part("only", true, 100, "<h2>Only</h2>"), part("engine", false, 50, "")]).is_empty());
    }
}
