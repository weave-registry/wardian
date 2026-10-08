//! The documentation site (/docs and /docs/<name>). The Markdown files in the repo are the only
//! source: they are built into the program (the assets) and rendered on request. `wardian docs`
//! writes the same pages as static files for the website, so both show one text.

use crate::ports::{
    assets::{Assets, DocPage},
    service::Pages,
};
use crate::usecases::{demos, skills};
use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::sync::{Arc, OnceLock};

/// Where each page's Markdown can be read and changed.
const SOURCE_BASE: &str = "https://github.com/weave-registry/wardian/blob/main/";
const REPO: &str = "https://github.com/weave-registry/wardian";

/// Who shows the page: Wardian itself, or the static website.
#[derive(Clone, Copy, PartialEq)]
pub enum Site {
    Host,
    Static,
}

pub struct Docs {
    assets: Arc<dyn Assets>,
    /// The search index of every page's headings, as JSON; the same for every page.
    index: OnceLock<String>,
}

impl Docs {
    pub fn new(assets: Arc<dyn Assets>) -> Docs {
        Docs { assets, index: OnceLock::new() }
    }

    /// A docs page as Wardian serves it, or None for an unknown name.
    pub fn page(&self, name: &str) -> Option<String> {
        self.render_page(name, Site::Host)
    }

    /// Every file of the static docs site, as (path, contents): the pages, the JSON Schemas, the
    /// component gallery with its files, at the paths Wardian serves them from, and the example
    /// apps the website runs and offers for download (ADR-2610081900).
    pub fn site(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for p in self.assets.docs() {
            let path = if p.name == "index" { "docs/index.html".to_string() } else { format!("docs/{}/index.html", p.name) };
            out.push((path, self.render_page(p.name, Site::Static).unwrap_or_default().into_bytes()));
        }
        for s in ["app.schema.json", "suite.schema.json"] {
            if let Some(body) = self.assets.schema(s) {
                out.push((format!("schemas/{s}"), body.as_bytes().to_vec()));
            }
        }
        out.push(("ui/index.html".to_string(), self.assets.gallery().as_bytes().to_vec()));
        for n in self.assets.ui_names() {
            if let Some(body) = self.assets.ui_file(n) {
                out.push((format!("ui/{n}"), body.as_bytes().to_vec()));
            }
        }
        out.extend(demos::site_files(&*self.assets));
        out
    }

    /// app.schema.json or suite.schema.json, for editors that check those files.
    pub fn schema(&self, name: &str) -> Option<&'static str> {
        self.assets.schema(name)
    }

    /// A component library file, as the gallery and `wardian add` users read it.
    pub fn ui_file(&self, name: &str) -> Option<&'static str> {
        self.assets.ui_file(name)
    }

    pub fn gallery(&self) -> &'static str {
        self.assets.gallery()
    }

    fn search_index(&self) -> &str {
        self.index.get_or_init(|| {
            let mut entries = Vec::new();
            for p in self.assets.docs() {
                let url = href(p.name);
                entries.push(serde_json::json!([p.title, "", url]));
                for (lvl, id, text) in render(p.md).1 {
                    if lvl == 2 || lvl == 3 {
                        entries.push(serde_json::json!([p.title, text, format!("{url}#{id}")]));
                    }
                }
            }
            // Inside a <script>, "</" must not end it early.
            serde_json::Value::Array(entries).to_string().replace("</", "<\\/")
        })
    }

    fn render_page(&self, name: &str, site: Site) -> Option<String> {
        let pages = self.assets.docs();
        let at = pages.iter().position(|p| p.name == name)?;
        let page = &pages[at];
        let (mut body, toc) = render(page.md);
        // On the website, the examples page opens with every example to try, and an example's page
        // shows the app running, and its download.
        if let (Site::Static, "examples", Some(end)) = (site, name, body.find("</h1>")) {
            body.insert_str(end + "</h1>".len(), &demos::gallery(&*self.assets));
        }
        if let (Site::Static, Some(app)) = (site, name.strip_prefix("examples/")) {
            if let (Some(boxed), Some(end)) = (demos::try_box(&*self.assets, app), body.find("</h1>")) {
                body.insert_str(end + "</h1>".len(), &boxed);
            }
        }
        Some(layout(pages, at, &body, &toc, self.search_index(), site, page))
    }
}

/// The address of a page: /docs for the home page, /docs/<name> for the others.
fn href(name: &str) -> String {
    if name == "index" {
        "/docs/".to_string()
    } else {
        format!("/docs/{name}")
    }
}

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
    let mut used = std::collections::HashSet::new();
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let lvl = match level {
                HeadingLevel::H1 => 1,
                HeadingLevel::H2 => 2,
                HeadingLevel::H3 => 3,
                HeadingLevel::H4 => 4,
                HeadingLevel::H5 => 5,
                HeadingLevel::H6 => 6,
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
            // A repeated heading gets "-1", "-2", ... so each id is unique.
            let base = slug(&text);
            let mut id = base.clone();
            let mut n = 0;
            while !used.insert(id.clone()) {
                n += 1;
                id = format!("{base}-{n}");
            }
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

/// The left column: every group and its pages, with the open page's sections under it.
fn navigation(pages: &[DocPage], at: usize, toc: &[(u8, String, String)]) -> String {
    // A page under another (group "") marks its parent, "examples" for "examples/life".
    let current = match pages[at].group {
        "" => pages[at].name.split('/').next().and_then(|parent| pages.iter().position(|p| p.name == parent)),
        _ => Some(at),
    };
    let mut nav = String::new();
    let mut group = "";
    for (i, p) in pages.iter().enumerate() {
        if p.group.is_empty() {
            continue;
        }
        if p.group != group {
            if !group.is_empty() {
                nav.push_str("</ul></div>");
            }
            group = p.group;
            nav.push_str(&format!("<div class=\"group\"><h4>{}</h4><ul>", escape(group)));
        }
        if Some(i) == current {
            nav.push_str(&format!("<li><a href=\"{}\" aria-current=\"page\">{}</a>", href(p.name), escape(p.title)));
            let toc: &[(u8, String, String)] = if i == at { toc } else { &[] };
            let sections: String = toc
                .iter()
                .filter(|(lvl, _, _)| *lvl == 2)
                .map(|(_, id, text)| format!("<li><a href=\"#{id}\">{}</a></li>", escape(text)))
                .collect();
            if !sections.is_empty() {
                nav.push_str(&format!("<ul class=\"toc\">{sections}</ul>"));
            }
            nav.push_str("</li>");
        } else {
            nav.push_str(&format!("<li><a href=\"{}\">{}</a></li>", href(p.name), escape(p.title)));
        }
    }
    nav.push_str("</ul></div>");
    nav
}

fn layout(pages: &[DocPage], at: usize, body: &str, toc: &[(u8, String, String)], index: &str, site: Site, page: &DocPage) -> String {
    let nav = navigation(pages, at, toc);
    let side = |p: Option<&DocPage>, rel: &str, label: &str| match p {
        Some(p) => format!("<a class=\"{rel}\" href=\"{}\" rel=\"{rel}\"><small>{label}</small>{}</a>", href(p.name), escape(p.title)),
        None => "<span></span>".to_string(),
    };
    // Previous and next walk the listed pages; a page under another leads back to it.
    let pager = if page.group.is_empty() {
        let parent = page.name.split('/').next().and_then(|n| pages.iter().find(|p| p.name == n));
        side(parent, "prev", "Back to")
    } else {
        let listed: Vec<&DocPage> = pages.iter().filter(|p| !p.group.is_empty()).collect();
        let i = listed.iter().position(|p| p.name == page.name).unwrap_or(0);
        format!("{}{}", side(i.checked_sub(1).map(|j| listed[j]), "prev", "Previous"), side(listed.get(i + 1).copied(), "next", "Next"))
    };
    let (home, home_label) = match site {
        Site::Host => ("/", "Open Wardian"),
        Site::Static => ("/", "Home"),
    };
    let github = if site == Site::Static { format!("<a href=\"{REPO}\">GitHub</a>") } else { String::new() };
    let title = if page.name == "index" { "Wardian documentation".to_string() } else { format!("{} · Wardian docs", page.title) };
    format!(
        r##"<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="Wardian documentation: {page_title}">
<link rel="icon" href="/logo.svg" type="image/svg+xml">
<style>
  :root {{ color-scheme: light dark; --bg: #f6f5ef; --panel: #fbfaf6; --ink: #183e32; --text: #23302a; --muted: #5f6b62; --line: #d8ddd2; --code: #ecefe4; --accent: #2f6b50; --mark: #dce8a5; }}
  @media (prefers-color-scheme: dark) {{ :root {{ --bg: #111814; --panel: #16201b; --ink: #e3efd9; --text: #d7ddd3; --muted: #9aa79c; --line: #2a3a31; --code: #1d2a23; --accent: #a9d28a; --mark: #3c4d1f; }} }}
  * {{ box-sizing: border-box; }}
  html {{ scroll-padding-top: 5rem; }}
  body {{ margin: 0; background: var(--bg); color: var(--text); font: 16px/1.65 system-ui, -apple-system, "Segoe UI", sans-serif; }}
  a {{ color: var(--accent); text-underline-offset: 3px; }}
  a:focus-visible, button:focus-visible, input:focus-visible, summary:focus-visible {{ outline: 3px solid #788d35; outline-offset: 2px; }}
  .skip {{ position: absolute; left: .5rem; top: -4rem; background: var(--panel); padding: .5rem; z-index: 5; }}
  .skip:focus {{ top: .5rem; }}
  header {{ display: flex; gap: 1rem; align-items: center; padding: .65rem 1.25rem; border-bottom: 1px solid var(--line); background: var(--panel); position: sticky; top: 0; z-index: 3; }}
  header .brand {{ display: flex; align-items: center; gap: .5rem; color: var(--ink); font-weight: 650; font-size: 1.15rem; letter-spacing: -.02em; text-decoration: none; white-space: nowrap; }}
  header .brand img {{ width: 26px; height: 26px; }}
  header .brand span {{ color: var(--muted); font-weight: 400; }}
  .search {{ position: relative; flex: 1; max-width: 26rem; margin-left: auto; }}
  .search input {{ width: 100%; font: inherit; font-size: .92rem; padding: .4rem .7rem; border: 1px solid var(--line); border-radius: 6px; background: var(--bg); color: var(--text); }}
  .search ol {{ position: absolute; top: 2.4rem; left: 0; right: 0; list-style: none; margin: 0; padding: .3rem; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 8px 30px rgb(0 0 0 / .15); max-height: 70vh; overflow: auto; }}
  .search ol[hidden] {{ display: none; }}
  .search li a {{ display: block; padding: .4rem .6rem; border-radius: 5px; color: var(--text); text-decoration: none; font-size: .9rem; }}
  .search li a small {{ display: block; color: var(--muted); }}
  .search li a[aria-selected="true"], .search li a:hover {{ background: var(--mark); }}
  header .links {{ display: flex; gap: 1rem; white-space: nowrap; }}
  header .links a {{ color: var(--muted); text-decoration: none; font-size: .92rem; }}
  header .links a:hover {{ color: var(--ink); }}
  .wrap {{ display: grid; grid-template-columns: 16rem minmax(0, 48rem); gap: 3rem; max-width: 72rem; margin: 0 auto; padding: 1.5rem 1.25rem 5rem; }}
  nav.side {{ position: sticky; top: 4.2rem; align-self: start; max-height: calc(100vh - 5rem); overflow: auto; font-size: .9rem; padding-bottom: 2rem; }}
  nav.side summary {{ display: none; }}
  nav.side h4 {{ margin: 1.1rem 0 .3rem; font-size: .72rem; letter-spacing: .12em; text-transform: uppercase; color: var(--muted); }}
  nav.side ul {{ list-style: none; margin: 0; padding: 0; }}
  nav.side a {{ display: block; padding: .18rem .5rem; color: var(--text); text-decoration: none; border-radius: 5px; }}
  nav.side a:hover {{ color: var(--accent); }}
  nav.side a[aria-current] {{ background: var(--mark); color: var(--ink); font-weight: 600; }}
  nav.side .toc {{ margin: .2rem 0 .4rem .6rem; border-left: 1px solid var(--line); }}
  nav.side .toc a {{ color: var(--muted); font-size: .84rem; padding: .1rem .6rem; }}
  main {{ min-width: 0; }}
  main h1 {{ font: 600 2.3rem/1.15 Georgia, "Times New Roman", serif; color: var(--ink); margin: .6rem 0 1.2rem; letter-spacing: -.01em; }}
  main h2 {{ font-size: 1.45rem; color: var(--ink); margin-top: 2.6rem; padding-top: .6rem; border-top: 1px solid var(--line); }}
  main h3 {{ font-size: 1.12rem; color: var(--ink); margin-top: 1.8rem; }}
  .anchor {{ margin-left: .4rem; color: var(--line); text-decoration: none; font-weight: 400; }}
  h2:hover .anchor, h3:hover .anchor {{ color: var(--accent); }}
  code {{ font: .88em ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--code); padding: .1em .3em; border-radius: 4px; }}
  pre {{ background: var(--code); padding: .9rem 1rem; border-radius: 8px; overflow-x: auto; line-height: 1.5; }}
  pre code {{ background: none; padding: 0; }}
  blockquote {{ margin: 1.2rem 0; padding: .6rem 1rem; border-left: 4px solid var(--mark); background: var(--panel); border-radius: 0 8px 8px 0; }}
  blockquote p {{ margin: .3rem 0; }}
  table {{ border-collapse: collapse; width: 100%; margin: 1rem 0; font-size: .93rem; display: block; overflow-x: auto; }}
  th, td {{ border-bottom: 1px solid var(--line); padding: .45rem .6rem; text-align: left; vertical-align: top; }}
  th {{ color: var(--muted); font-weight: 600; }}
  hr {{ border: 0; border-top: 1px solid var(--line); margin: 2rem 0; }}
  .edit {{ margin-top: 3rem; font-size: .88rem; color: var(--muted); }}
  .pager {{ display: flex; justify-content: space-between; gap: 1rem; margin-top: 1rem; }}
  .pager a {{ flex: 1; max-width: 48%; padding: .7rem 1rem; border: 1px solid var(--line); border-radius: 8px; text-decoration: none; color: var(--ink); background: var(--panel); }}
  .pager a:hover {{ border-color: var(--accent); }}
  .pager a small {{ display: block; color: var(--muted); font-size: .78rem; }}
  .pager .next {{ text-align: right; margin-left: auto; }}
  .try {{ margin: 1rem 0 2rem; padding: 1rem; border: 1px solid var(--line); border-radius: 8px; background: var(--panel); }}
  .try h2 {{ margin-top: 0; padding-top: 0; border-top: 0; }}
  .try-frame {{ width: 100%; height: 560px; border: 1px solid var(--line); border-radius: 6px; background: #fff; }}
  .try .muted {{ color: var(--muted); font-size: .88rem; }}
  .gallery {{ margin: 1rem 0 2.5rem; }}
  .gallery h2 {{ margin-top: 0; padding-top: 0; border-top: 0; }}
  .gallery ul {{ list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr)); gap: .8rem; }}
  .gallery li {{ display: flex; flex-direction: column; justify-content: space-between; gap: .6rem; padding: .9rem 1rem; border: 1px solid var(--line); border-radius: 8px; background: var(--panel); }}
  .gallery .card-link {{ display: grid; gap: .2rem; text-decoration: none; color: var(--ink); }}
  .gallery .card-link small {{ color: var(--muted); text-transform: uppercase; letter-spacing: .06em; font-size: .7rem; }}
  .gallery .card-link span {{ color: var(--muted); font-size: .88rem; }}
  .gallery .go {{ align-self: flex-start; padding: .35rem .9rem; border-radius: 6px; background: var(--accent); color: #fff; text-decoration: none; font-weight: 600; }}
  .gallery .needs {{ color: var(--muted); font-size: .85rem; }}
  @media (max-width: 900px) {{
    .wrap {{ grid-template-columns: minmax(0, 1fr); gap: 0; padding-top: .5rem; }}
    nav.side {{ position: static; max-height: none; border-bottom: 1px solid var(--line); padding-bottom: .5rem; margin-bottom: 1rem; }}
    nav.side summary {{ display: list-item; cursor: pointer; padding: .5rem 0; color: var(--ink); font-weight: 600; }}
    header .brand span, header .links a:not(:first-child) {{ display: none; }}
  }}
</style>
<a class="skip" href="#content">Skip to content</a>
<header>
  <a class="brand" href="/docs/"><img src="/logo.svg" alt="">Wardian <span>docs</span></a>
  <div class="search" role="search">
    <input id="q" type="search" placeholder="Search the docs  ( / )" aria-label="Search the docs" autocomplete="off" aria-controls="hits" aria-expanded="false">
    <ol id="hits" role="listbox" hidden></ol>
  </div>
  <div class="links"><a href="{home}">{home_label}</a>{github}</div>
</header>
<div class="wrap">
<nav class="side" aria-label="Documentation"><details id="menu" open><summary>Menu</summary>{nav}</details></nav>
<main id="content">
{body}
<p class="edit">This page is <a href="{source_base}{source}"><code>{source}</code></a> in the repository. Something wrong or missing? Change that file.</p>
<div class="pager">{pager}</div>
</main>
</div>
<script>
(() => {{
  const index = {index};
  const q = document.getElementById('q'), hits = document.getElementById('hits');
  const menu = document.getElementById('menu');
  if (matchMedia('(max-width: 900px)').matches) menu.open = false;
  let sel = -1;
  const esc = s => s.replace(/[&<>"]/g, c => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}})[c]);
  function show() {{
    const words = q.value.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) {{ hits.hidden = true; q.setAttribute('aria-expanded', 'false'); return; }}
    const found = index.map(([page, head, url]) => {{
      const hay = (page + ' ' + head).toLowerCase();
      if (!words.every(w => hay.includes(w))) return null;
      const score = words.reduce((s, w) => s + (head.toLowerCase().includes(w) ? 2 : 0) + (page.toLowerCase().includes(w) ? 1 : 0), head ? 0 : 3);
      return {{ page, head, url, score }};
    }}).filter(Boolean).sort((a, b) => b.score - a.score).slice(0, 12);
    sel = found.length ? 0 : -1;
    hits.innerHTML = found.length
      ? found.map((h, i) => `<li><a role="option" href="${{esc(h.url)}}" aria-selected="${{i === 0}}">${{esc(h.head || h.page)}}<small>${{esc(h.head ? h.page : 'Page')}}</small></a></li>`).join('')
      : '<li><a role="option" aria-disabled="true">Nothing found</a></li>';
    hits.hidden = false; q.setAttribute('aria-expanded', 'true');
  }}
  function move(d) {{
    const links = [...hits.querySelectorAll('a[href]')];
    if (!links.length) return;
    sel = (sel + d + links.length) % links.length;
    links.forEach((a, i) => a.setAttribute('aria-selected', String(i === sel)));
    links[sel].scrollIntoView({{ block: 'nearest' }});
  }}
  q.addEventListener('input', show);
  q.addEventListener('keydown', e => {{
    if (e.key === 'ArrowDown') {{ e.preventDefault(); move(1); }}
    else if (e.key === 'ArrowUp') {{ e.preventDefault(); move(-1); }}
    else if (e.key === 'Enter') {{ const a = hits.querySelectorAll('a[href]')[sel]; if (a) location.href = a.href; }}
    else if (e.key === 'Escape') {{ q.value = ''; show(); q.blur(); }}
  }});
  document.addEventListener('keydown', e => {{
    if (e.key === '/' && document.activeElement !== q && !/input|textarea/i.test(document.activeElement.tagName)) {{ e.preventDefault(); q.focus(); }}
  }});
  document.addEventListener('click', e => {{ if (!e.target.closest('.search')) {{ hits.hidden = true; q.setAttribute('aria-expanded', 'false'); }} }});
}})();
</script>
</html>
"##,
        page_title = escape(page.title),
        source_base = SOURCE_BASE,
        source = page.source,
    )
}

impl Pages for Docs {
    fn page(&self, name: &str) -> Option<String> {
        Docs::page(self, name)
    }
    fn site(&self) -> Vec<(String, Vec<u8>)> {
        Docs::site(self)
    }
    fn schema(&self, name: &str) -> Option<&'static str> {
        Docs::schema(self, name)
    }
    fn ui_file(&self, name: &str) -> Option<&'static str> {
        Docs::ui_file(self, name)
    }
    fn gallery(&self) -> &'static str {
        Docs::gallery(self)
    }
    fn skill_names(&self) -> Vec<&'static str> {
        skills::names(&*self.assets)
    }
    fn skills(&self) -> Vec<(String, String)> {
        skills::files(&*self.assets)
    }
}

#[cfg(test)]
mod tests {
    use super::{render, slug};

    #[test]
    fn slugs_for_headings() {
        assert_eq!(slug("6.2. `suite.json`"), "6-2-suite-json");
    }

    #[test]
    fn repeated_headings_get_unique_ids() {
        let (body, toc) = render("## 1.0\n### Added\n## 0.9\n### Added\n### Added 1\n");
        let ids: Vec<&str> = toc.iter().map(|(_, id, _)| id.as_str()).collect();
        assert_eq!(ids, ["1-0", "added", "0-9", "added-1", "added-1-1"]);
        assert_eq!(body.matches("id=\"added\"").count(), 1);
        assert!(body.contains("href=\"#added-1\""));
    }

    #[test]
    fn deep_headings_keep_their_level() {
        let (body, toc) = render("#### Four\n##### Five\n###### Six\n");
        assert!(body.contains("<h4 id=\"four\">Four</h4>"));
        assert!(body.contains("<h5 id=\"five\">Five</h5>"));
        assert!(body.contains("<h6 id=\"six\">Six</h6>"));
        assert!(toc.is_empty());
    }
}
