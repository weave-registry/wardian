//! The docs pages (/docs/guide, /docs/spec) and the JSON Schemas
//! (/schemas/*.json). The Markdown files in the repo are the only source:
//! they are built into the program and rendered on request.

use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

const GUIDE_MD: &str = include_str!("../GUIDE.md");
const SPEC_MD: &str = include_str!("../SPEC.md");
pub const APP_SCHEMA: &str = include_str!("../schemas/app.schema.json");
pub const SUITE_SCHEMA: &str = include_str!("../schemas/suite.schema.json");

const PAGES: &[(&str, &str, &str)] = &[
    ("guide", "Building Wardian apps", GUIDE_MD),
    ("spec", "Package format", SPEC_MD),
];

/// "6.2. `suite.json`" -> "6-2-suite-json", for links to a heading.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Renders Markdown, giving h2/h3 headings an id. Returns the HTML and the
/// table of contents as (level, id, text).
fn render(md: &str) -> (String, Vec<(u8, String, String)>) {
    let events: Vec<Event> = Parser::new_ext(md, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH).collect();
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut toc = Vec::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let lvl = match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                _ => 4,
            };
            // Collect the heading's text and inner events up to its end.
            let mut j = i + 1;
            let mut text = String::new();
            let mut inner = Vec::new();
            while !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                if let Event::Text(t) | Event::Code(t) = &events[j] {
                    text.push_str(t);
                }
                inner.push(events[j].clone());
                j += 1;
            }
            let id = slug(&text);
            if (2..=3).contains(&lvl) {
                toc.push((lvl, id.clone(), text.clone()));
            }
            out.push(Event::Html(CowStr::from(format!("<h{lvl} id=\"{id}\">"))));
            out.extend(inner);
            if (2..=3).contains(&lvl) {
                out.push(Event::Html(CowStr::from(format!("<a class=\"anchor\" href=\"#{id}\" aria-label=\"Link to this section\">#</a>"))));
            }
            out.push(Event::Html(CowStr::from(format!("</h{lvl}>\n"))));
            i = j + 1;
            continue;
        }
        out.push(events[i].clone());
        i += 1;
    }
    let mut body = String::new();
    html::push_html(&mut body, out.into_iter());
    (body, toc)
}

/// A docs page, or None for an unknown name.
pub fn page(name: &str) -> Option<String> {
    let (_, title, md) = PAGES.iter().find(|(n, _, _)| *n == name)?;
    let (body, toc) = render(md);
    let nav: String = PAGES
        .iter()
        .map(|(n, t, _)| format!("<a href=\"/docs/{n}\"{}>{}</a>", if *n == name { " aria-current=\"page\"" } else { "" }, escape(t)))
        .collect::<Vec<_>>()
        .join("");
    let toc: String = toc
        .iter()
        .map(|(lvl, id, text)| format!("<li class=\"l{lvl}\"><a href=\"#{id}\">{}</a></li>", escape(text)))
        .collect();
    Some(format!(
        r#"<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Wardian</title>
<link rel="icon" href="/logo.svg" type="image/svg+xml">
<style>
  :root {{ color-scheme: light dark; --bg: #faf7f2; --panel: #fff; --ink: #22201c; --muted: #6f675c; --line: #e8e1d6; --code: #f3eee6; --accent: #b9471f; }}
  @media (prefers-color-scheme: dark) {{ :root {{ --bg: #16130f; --panel: #1f1b16; --ink: #ede7de; --muted: #a39a8c; --line: #342d25; --code: #2a241e; --accent: #f08a5d; }} }}
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; background: var(--bg); color: var(--ink); font: 16px/1.6 system-ui, sans-serif; }}
  header {{ display: flex; gap: 1.25rem; align-items: center; padding: .7rem 1.25rem; border-bottom: 1px solid var(--line); background: var(--panel); position: sticky; top: 0; z-index: 1; }}
  header .brand {{ display: flex; align-items: center; gap: .45rem; color: var(--ink); font-weight: 750; font-size: 1.15rem; letter-spacing: -.03em; }}
  header .brand img {{ width: 24px; height: 24px; }}
  header nav {{ display: flex; gap: 1rem; }}
  header a {{ color: var(--muted); text-decoration: none; }}
  header a[aria-current] {{ color: var(--ink); font-weight: 600; }}
  header .home {{ margin-left: auto; }}
  .wrap {{ display: grid; grid-template-columns: 15rem minmax(0, 46rem); gap: 2.5rem; max-width: 66rem; margin: 0 auto; padding: 1.5rem 1.25rem 5rem; }}
  aside {{ position: sticky; top: 4rem; align-self: start; max-height: calc(100vh - 5rem); overflow: auto; font-size: .88rem; }}
  aside ul {{ list-style: none; margin: 0; padding: 0; }}
  aside li.l3 {{ padding-left: .9rem; }}
  aside a {{ color: var(--muted); text-decoration: none; display: block; padding: .15rem 0; }}
  aside a:hover {{ color: var(--accent); }}
  main h1 {{ font-size: 2rem; line-height: 1.2; margin: .5rem 0 1rem; }}
  main h2 {{ margin-top: 2.5rem; padding-top: .5rem; border-top: 1px solid var(--line); }}
  main h2, main h3 {{ scroll-margin-top: 4.5rem; }}
  .anchor {{ margin-left: .4rem; color: var(--line); text-decoration: none; font-weight: 400; }}
  h2:hover .anchor, h3:hover .anchor {{ color: var(--accent); }}
  a {{ color: var(--accent); }}
  code {{ font: .9em ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--code); padding: .1em .3em; border-radius: 4px; }}
  pre {{ background: var(--code); padding: .9rem 1rem; border-radius: 8px; overflow-x: auto; }}
  pre code {{ background: none; padding: 0; }}
  table {{ border-collapse: collapse; width: 100%; margin: 1rem 0; font-size: .94rem; display: block; overflow-x: auto; }}
  th, td {{ border-bottom: 1px solid var(--line); padding: .45rem .6rem; text-align: left; vertical-align: top; }}
  th {{ color: var(--muted); font-weight: 600; }}
  hr {{ border: 0; border-top: 1px solid var(--line); margin: 2rem 0; }}
  @media (max-width: 860px) {{ .wrap {{ grid-template-columns: minmax(0, 1fr); }} aside {{ display: none; }} }}
</style>
<header><a class="brand" href="/docs"><img src="/logo.svg" alt="">Wardian docs</a><nav>{nav}</nav><a class="home" href="/">Open Wardian</a></header>
<div class="wrap">
<aside><ul>{toc}</ul></aside>
<main>
{body}
</main>
</div>
</html>
"#,
        title = escape(title)
    ))
}

#[cfg(test)]
mod tests {
    use super::{page, slug};

    #[test]
    fn docs_render_with_anchors() {
        assert_eq!(slug("6.2. `suite.json`"), "6-2-suite-json");
        let spec = page("spec").unwrap();
        assert!(spec.contains("id=\"6-5-ctx\"") && spec.contains("<table>"));
        assert!(page("guide").unwrap().contains("wardian new module"));
        assert!(page("nope").is_none());
    }
}
