//! "Make an app": a chat in which Claude writes an app's files.
//!
//! Claude never touches the apps folder directly. It works on a copy of the
//! package held in memory, through a few tools (list, read, write, delete,
//! check, finish). Only `finish` saves, and only when `wardian check` passes:
//! the files go to a hidden staging folder, are checked there, and then
//! replace the app in one rename. The version they replace goes to the trash,
//! so every AI change can be undone with Restore.
//!
//! Claude cannot compile anything here, so AI-made apps are plain
//! JavaScript, HTML and CSS: a page app (Wardian adds an empty app.wasm, the
//! marker that makes a folder an app) or a suite.

use crate::check::check_dir;
use crate::hub::{write_private, Hub};
use crate::source::{safe_rel, safe_segment, unix_now, APP_MARKERS, SKIP_DIRS};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

const DEFAULT_MODEL: &str = "claude-opus-5-5";
const DEFAULT_BASE: &str = "https://api.anthropic.com";
/// Model requests in one turn of the chat, before Wardian stops it.
const MAX_STEPS: usize = 40;
const MAX_TOKENS: u32 = 16_000;
const MAX_MESSAGE_CHARS: usize = 20_000;
const MAX_FILE_CHARS: usize = 512 * 1024;
const MAX_FILES: usize = 300;
const MAX_SESSIONS: usize = 20;
/// The smallest valid WebAssembly module: the marker for a page app with no
/// WebAssembly of its own.
const EMPTY_WASM: &[u8] = b"\0asm\x01\0\0\0";

const SPEC_MD: &str = include_str!("../SPEC.md");
const EXAMPLES: &[(&str, &str)] = &[
    ("suite.json", include_str!("../templates/suite/suite.json")),
    ("style.css", include_str!("../templates/suite/style.css")),
    ("header.html", include_str!("../templates/suite/header.html")),
    ("apps/input/app.js", include_str!("../templates/suite/apps/input/app.js")),
    ("apps/input/view.html", include_str!("../templates/suite/apps/input/view.html")),
    ("apps/output/app.js", include_str!("../templates/suite/apps/output/app.js")),
    ("apps/output/view.html", include_str!("../templates/suite/apps/output/view.html")),
    ("apps/text/app.js", include_str!("../templates/suite/apps/text/app.js")),
];

const INTRO: &str = r#"You build apps for Wardian, a program that runs small web apps in a sandbox. The person you talk to describes an app; you write its files. Many users are not programmers: keep your chat replies short, friendly and free of jargon.

You work on the files of ONE package through tools: list_files, read_file, write_file, delete_file, add_components, check and finish. Nothing is saved until finish succeeds, and finish runs `wardian check` first. Each finish saves a new version; the old one goes to the trash, so the user can undo.

Hard limits:
- You cannot compile Rust or WebAssembly, run npm, or write binary files. Write the app in plain JavaScript, HTML and CSS, with no build step.
- Use no external URLs: no CDN scripts, no remote fonts or images, no fetch to other sites. Put everything in the package. Draw graphics with SVG or canvas.
- `claude:sample` (AI inside the app) works in Wardian only when an Anthropic key is saved, and only after the user allows it. `ctx.cap('sample')` can resolve to null: build the app so it still works without AI.

Choose the kind:
- **page** (most apps): app.json with "format": 1, "title", "description" and "page": "index.html", plus index.html and its .js/.css files. Do NOT write app.wasm: Wardian adds an empty one, which marks the folder as an app. The page runs sandboxed on an opaque origin: localStorage, sessionStorage, IndexedDB and cookies throw an error there. Do not use them. To keep data, offer download and upload of a file.
- **suite**: when the user asks for separate parts, or the app has clearly separate parts that share data. Follow section 6 of the spec exactly. Each app's contract appears twice, in suite.json and in Kernel.register, and the two must match or the kernel refuses to start the app. Frames have no network. Use ctx.store (capability "storage") to keep data, and ctx.cap('downloads') (capability "claude:downloads") to save files.

Use the Wardian component library. Every Wardian app does, so they all look and work alike:
- Call add_components first, with what the app needs (always at least button, field and card). It copies the files into ui/ and returns how to use each one. For a suite it lists them in suite.json; for a page app it returns the <link> and <script> tags to put in the page's <head>.
- Use the library's classes (w-button, w-input, w-card, w-table …) for every control. Do not write your own button, input or card styles. Take colours, spacing and fonts from its tokens (var(--w-fg), var(--w-border) …) in your own CSS.
- A page app also gets "arrange": mark each part of the page with data-panel, so each viewer can reorder, move and hide the parts. A suite gets Arrange from Wardian without this.

Make it good:
- A clean, modern, calm design. Responsive to narrow screens. Light and dark themes through prefers-color-scheme. Labels on every input. Realistic sample data, never lorem ipsum.
- Handle bad input without crashing; show a short message instead.
- Plain readable code with a comment at the top of each file that says what it does.

How to work:
- New app: say in one or two sentences what you will build. Write all files. Call check and fix every error. Then call finish with a short lowercase name (letters, digits, '-') and a one-sentence summary for the user.
- Change to an existing app: list_files, read what you need, change only what the request needs, check, finish.
- A message that starts with "[browser test]" comes from Wardian, which ran your app in a real browser. Fix what it reports, then finish again.
- After finish, reply in one or two sentences: what the app does and how to use it.

The package format specification follows, then a complete working example suite."#;

fn tools() -> Value {
    let path = json!({ "type": "string", "description": "Path inside the package, like \"index.html\" or \"apps/chart/app.js\". Letters, digits, '-', '_', '.' and '/'." });
    json!([
        { "name": "list_files", "description": "List the package's files with their sizes.",
          "input_schema": { "type": "object", "properties": {} } },
        { "name": "read_file", "description": "Read one text file from the package.",
          "input_schema": { "type": "object", "properties": { "path": path }, "required": ["path"] } },
        { "name": "write_file", "description": "Create or replace a text file. For a very large file, write the first part, then add the rest with append: true.",
          "input_schema": { "type": "object", "properties": {
              "path": path,
              "content": { "type": "string" },
              "append": { "type": "boolean", "description": "Add to the end of the file instead of replacing it." } },
            "required": ["path", "content"] } },
        { "name": "delete_file", "description": "Delete one file from the package.",
          "input_schema": { "type": "object", "properties": { "path": path }, "required": ["path"] } },
        { "name": "add_components", "description": "Copy components of the Wardian component library into ui/ (theme.css always comes too) and say how to use them. Components: button, field, card, badge, table, switch, tabs, dialog, toast, tooltip, progress, arrange.",
          "input_schema": { "type": "object", "properties": {
              "components": { "type": "array", "items": { "type": "string" } } },
            "required": ["components"] } },
        { "name": "check", "description": "Run `wardian check` on the package as it is now. Returns errors and warnings.",
          "input_schema": { "type": "object", "properties": {} } },
        { "name": "finish", "description": "Check the package and, if it passes, save it so the user can open it. If the check fails, nothing is saved and you get the errors.",
          "input_schema": { "type": "object", "properties": {
              "name": { "type": "string", "description": "For a new app: the folder name, lowercase, like \"habit-tracker\". Ignored when changing an existing app." },
              "summary": { "type": "string", "description": "One sentence for the user: what changed." } },
            "required": ["summary"] } }
    ])
}

fn system() -> Value {
    let mut reference = format!("<spec>\n{SPEC_MD}\n</spec>\n\n<example-suite>\n");
    for (path, body) in EXAMPLES {
        reference += &format!("<file path=\"{path}\">\n{body}\n</file>\n");
    }
    reference += "</example-suite>\n(The example's \"text\" app loads a .wasm file with ctx.asset. Your apps cannot include .wasm files, so do that work in JavaScript.)";
    json!([
        { "type": "text", "text": INTRO },
        // Tools and system come first in the request, so this marker caches both.
        { "type": "text", "text": reference, "cache_control": { "type": "ephemeral" } }
    ])
}

/// Where to reach the API, and with which key.
#[derive(Clone)]
struct Api {
    key: String,
    /// Sent as anthropic-workspace-id. Keys that are not scoped to one workspace need it.
    workspace: String,
    base: String,
    model: String,
}

impl Api {
    fn agent() -> ureq::Agent {
        ureq::AgentBuilder::new().timeout(Duration::from_secs(600)).build()
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        let req = Self::agent()
            .request(method, &format!("{}{path}", self.base))
            .set("x-api-key", &self.key)
            .set("anthropic-version", "2023-06-01");
        if self.workspace.is_empty() {
            req
        } else {
            req.set("anthropic-workspace-id", &self.workspace)
        }
    }

    /// One model request. Retries rate limits and overloads a few times.
    fn messages(&self, messages: &[Value], cancel: &AtomicBool) -> Result<Value, String> {
        let mut messages = messages.to_vec();
        // Cache the conversation so far: the next request reuses it.
        if let Some(Value::Array(blocks)) = messages.last_mut().map(|m| &mut m["content"]) {
            if let Some(Value::Object(last)) = blocks.last_mut() {
                last.insert("cache_control".into(), json!({ "type": "ephemeral" }));
            }
        }
        let body = json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": system(),
            "tools": tools(),
            "messages": messages,
        });
        let mut wait = 2;
        loop {
            match self.request("POST", "/v1/messages").send_json(body.clone()) {
                Ok(resp) => return resp.into_json().map_err(|e| format!("unreadable reply from the API: {e}")),
                Err(ureq::Error::Status(code, resp)) if matches!(code, 429 | 500 | 502 | 503 | 529) && wait <= 16 => {
                    drop(resp);
                    for _ in 0..wait * 10 {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("stopped".into());
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                    wait *= 2;
                }
                Err(e) => return Err(api_error(e)),
            }
        }
    }

    /// Asks for one model's details: a cheap way to test a key.
    fn test(&self) -> Result<(), String> {
        self.request("GET", &format!("/v1/models/{}", self.model)).call().map(drop).map_err(api_error)
    }
}

/// The most an app may send in one `claude:sample` prompt.
const MAX_SAMPLE_CHARS: usize = 60_000;
/// The model for `{modelTier: 'quick'}` requests.
const QUICK_MODEL: &str = "claude-haiku-4-5-20251001";

impl Api {
    /// One question from an app (`claude:sample`), no tools. Errors start with a
    /// code the app can act on ("rate_limited: …"), the same codes the Claude
    /// viewer uses.
    fn sample(&self, prompt: &str, quick: bool, max_tokens: u32) -> Result<Value, String> {
        let model = if quick { QUICK_MODEL } else { self.model.as_str() };
        let body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let mut wait = 2;
        let resp: Value = loop {
            match self.request("POST", "/v1/messages").send_json(body.clone()) {
                Ok(r) => break r.into_json().map_err(|e| format!("error: unreadable reply from the API: {e}"))?,
                Err(ureq::Error::Status(code, r)) if matches!(code, 500 | 502 | 503 | 529) && wait <= 8 => {
                    drop(r);
                    thread::sleep(Duration::from_secs(wait));
                    wait *= 2;
                }
                Err(ureq::Error::Status(429, _)) => return Err("rate_limited: the Anthropic API rate limit was reached".into()),
                Err(ureq::Error::Status(401, _)) => return Err("not_granted: the Anthropic API key was refused; check Settings".into()),
                Err(e) => return Err(format!("error: {}", api_error(e))),
            }
        };
        let text: String = resp["content"]
            .as_array()
            .map(|blocks| blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(""))
            .unwrap_or_default();
        let stop = resp["stop_reason"].as_str().unwrap_or("");
        if stop == "refusal" {
            return Err("refused: Claude declined this request".into());
        }
        Ok(json!({ "text": text, "truncated": stop == "max_tokens", "model": model }))
    }
}

/// The first JSON object or array in a model's answer, which may wrap it in
/// a code fence or a sentence.
fn json_in(text: &str) -> Option<Value> {
    let start = text.find(['{', '['])?;
    let end = text.rfind(['}', ']'])?;
    (end > start).then(|| serde_json::from_str(&text[start..=end]).ok()).flatten()
}

fn api_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: Value = resp.into_json().unwrap_or_default();
            let msg = body["error"]["message"].as_str().unwrap_or("");
            match code {
                401 => "the API key was refused (401). Check it in Settings.".into(),
                _ if msg.is_empty() => format!("the API answered HTTP {code}"),
                _ => format!("the API answered HTTP {code}: {msg}"),
            }
        }
        ureq::Error::Transport(t) => format!("cannot reach the API: {}", t.message().unwrap_or(&t.kind().to_string())),
    }
}

/// One chat. It lives in memory only; the apps it saves are ordinary folders.
struct Session {
    /// The app this chat creates or changes; None until the first save of a new app.
    app: Option<String>,
    messages: Vec<Value>,
    /// What the user sees: their messages, Claude's words, each tool step.
    events: Vec<Value>,
    busy: bool,
    cancel: Arc<AtomicBool>,
    last_used: u64,
    /// A browser tab that is testing the latest save: (index of the saved event, when it claimed it).
    /// Any tab may run the test, so the chat goes on while you work elsewhere; the claim keeps two
    /// tabs from testing the same save. A claim older than TEST_CLAIM_SECS is given up.
    test_claim: Option<(usize, u64)>,
}

/// How long a tab may hold a test before another tab may take it over (the tab may have closed).
const TEST_CLAIM_SECS: u64 = 90;

pub struct Studio {
    key_path: PathBuf,
    key: Mutex<Option<(String, &'static str)>>,
    workspace_path: PathBuf,
    workspace: Mutex<String>,
    base: String,
    model: String,
    sessions: Mutex<HashMap<String, Arc<Mutex<Session>>>>,
}

impl Studio {
    pub fn new(data_dir: &Path) -> Studio {
        let key_path = data_dir.join("anthropic-key");
        let saved = fs::read_to_string(&key_path).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
        let key = match saved {
            Some(k) => Some((k, "settings")),
            None => env("ANTHROPIC_API_KEY").map(|k| (k, "environment")),
        };
        let workspace_path = data_dir.join("anthropic-workspace");
        let workspace = fs::read_to_string(&workspace_path)
            .ok()
            .map(|w| w.trim().to_string())
            .filter(|w| !w.is_empty())
            .or_else(|| env("ANTHROPIC_WORKSPACE_ID"))
            .unwrap_or_default();
        Studio {
            key_path,
            key: Mutex::new(key),
            workspace_path,
            workspace: Mutex::new(workspace),
            base: env("ANTHROPIC_BASE_URL").unwrap_or_else(|| DEFAULT_BASE.into()).trim_end_matches('/').to_string(),
            model: env("WARDIAN_AI_MODEL").or_else(|| env("RUSTLE_AI_MODEL")).unwrap_or_else(|| DEFAULT_MODEL.into()),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn api(&self) -> Result<Api, String> {
        let key = self.key.lock().unwrap().as_ref().map(|(k, _)| k.clone());
        let key = key.ok_or("no Anthropic API key yet: add one in Settings → Make apps with Claude")?;
        Ok(Api { key, workspace: self.workspace.lock().unwrap().clone(), base: self.base.clone(), model: self.model.clone() })
    }

    pub fn status(&self) -> Value {
        let key = self.key.lock().unwrap();
        let ws = self.workspace.lock().unwrap();
        json!({ "ready": key.is_some(), "key_from": key.as_ref().map(|(_, from)| *from), "model": self.model,
                "workspace": if ws.is_empty() { Value::Null } else { json!(*ws) } })
    }

    /// Answers one `claude:sample` request from an app. `body`: {prompt, json, tier}.
    pub fn sample(&self, body: &Value) -> Result<Value, String> {
        let api = self.api().map_err(|e| format!("not_granted: {e}"))?;
        let prompt = body["prompt"].as_str().unwrap_or("").trim();
        if prompt.is_empty() {
            return Err("error: the prompt is empty".into());
        }
        if prompt.chars().count() > MAX_SAMPLE_CHARS {
            return Err("prompt_too_large: the prompt is too long".into());
        }
        let quick = body["tier"].as_str() == Some("quick");
        if body["json"].as_bool().unwrap_or(false) {
            let ask = format!("{prompt}\n\nAnswer with the JSON only, no other words.");
            let out = api.sample(&ask, quick, 2_000)?;
            let value = json_in(out["text"].as_str().unwrap_or("")).ok_or("invalid_json: the answer was not JSON")?;
            Ok(json!({ "value": value }))
        } else {
            api.sample(prompt, quick, 4_000)
        }
    }

    /// Tests a key against the API before saving it, so a bad key never
    /// replaces a good one. An empty key removes the saved one.
    ///
    /// `workspace` (when given) is the workspace ID for keys that are not
    /// scoped to one workspace; an empty one removes it. With an empty key
    /// and a workspace, the saved key is tested again with the new workspace.
    pub fn set_key(&self, key: &str, workspace: Option<&str>) -> Result<Value, String> {
        let key = key.trim();
        let workspace = match workspace.map(str::trim) {
            Some(w) if w.len() > 100 || !w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') => {
                return Err("that does not look like a workspace ID (letters, digits, '_' and '-')".into())
            }
            w => w.map(String::from),
        };
        if key.is_empty() && workspace.is_none() {
            let _ = fs::remove_file(&self.key_path);
            *self.key.lock().unwrap() = env("ANTHROPIC_API_KEY").map(|k| (k, "environment"));
            return Ok(self.status());
        }
        let typed = !key.is_empty();
        let key = if typed {
            if key.len() > 300 || !key.chars().all(|c| c.is_ascii_graphic()) {
                return Err("that does not look like an API key".into());
            }
            key.to_string()
        } else {
            self.key.lock().unwrap().as_ref().map(|(k, _)| k.clone()).ok_or("paste the API key too")?
        };
        let ws = workspace.clone().unwrap_or_else(|| self.workspace.lock().unwrap().clone());
        Api { key: key.clone(), workspace: ws.clone(), base: self.base.clone(), model: self.model.clone() }.test()?;
        if typed {
            write_private(&self.key_path, key.as_bytes()).map_err(|e| format!("saving the key: {e}"))?;
            *self.key.lock().unwrap() = Some((key, "settings"));
        }
        if workspace.is_some() {
            if ws.is_empty() {
                let _ = fs::remove_file(&self.workspace_path);
            } else {
                write_private(&self.workspace_path, ws.as_bytes()).map_err(|e| format!("saving the workspace: {e}"))?;
            }
            *self.workspace.lock().unwrap() = ws;
        }
        Ok(self.status())
    }

    /// Adds the user's message to a chat (a new one when `session` is
    /// missing) and starts Claude's turn in the background.
    pub fn send(self: &Arc<Self>, hub: &Arc<Hub>, body: &Value) -> Result<Value, String> {
        let api = self.api()?;
        if !hub.serving_local() {
            return Err("AI apps are saved in the local apps folder. Switch the source to Local first.".into());
        }
        let text = body["message"].as_str().unwrap_or("").trim();
        if text.is_empty() {
            return Err("write what the app should do".into());
        }
        if text.len() > MAX_MESSAGE_CHARS {
            return Err("that message is too long".into());
        }
        let (id, session) = match body["session"].as_str().filter(|s| !s.is_empty()) {
            Some(id) => {
                let s = self.sessions.lock().unwrap().get(id).cloned();
                (id.to_string(), s.ok_or("that chat has ended (Wardian restarted); start a new one")?)
            }
            None => {
                let app = body["app"].as_str().filter(|a| !a.is_empty());
                if let Some(app) = app {
                    if !safe_segment(app) || !is_app(&hub.local_root().join(app)) {
                        return Err(format!("no local app \"{app}\" to change"));
                    }
                }
                let id = format!("{:x}{:08x}", unix_now(), rand32());
                let s = Arc::new(Mutex::new(Session {
                    app: app.map(String::from),
                    messages: Vec::new(),
                    events: Vec::new(),
                    busy: false,
                    cancel: Arc::new(AtomicBool::new(false)),
                    last_used: unix_now(),
                    test_claim: None,
                }));
                self.remember(&id, Arc::clone(&s));
                (id, s)
            }
        };

        let mut s = session.lock().unwrap();
        if s.busy {
            return Err("Claude is still working on the last message".into());
        }
        let mut said = text.to_string();
        if s.messages.is_empty() {
            if let Some(app) = &s.app {
                said = format!("I want to change the existing app \"{app}\".\n\n{said}");
            }
        }
        // After a save, the last message holds finish's result; a new user
        // turn must join it, since two user messages in a row are not allowed.
        match s.messages.last_mut() {
            Some(m) if m["role"] == "user" => {
                if let Some(blocks) = m["content"].as_array_mut() {
                    blocks.push(json!({ "type": "text", "text": said }));
                }
            }
            _ => s.messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": said }] })),
        }
        let kind = if text.starts_with("[browser test]") { "test" } else { "user" };
        s.events.push(json!({ "kind": kind, "text": text }));
        s.busy = true;
        s.last_used = unix_now();
        s.cancel.store(false, Ordering::Relaxed);
        let cancel = Arc::clone(&s.cancel);
        drop(s);

        let hub = Arc::clone(hub);
        let worker = Arc::clone(&session);
        thread::spawn(move || {
            let result = run_turn(&api, &hub, &worker, &cancel);
            let mut s = worker.lock().unwrap();
            match result {
                Ok(()) => {}
                Err(e) if e == "stopped" => s.events.push(json!({ "kind": "stopped", "text": "Stopped. Nothing after the last save was kept." })),
                Err(e) => s.events.push(json!({ "kind": "error", "text": e })),
            }
            s.busy = false;
        });
        Ok(json!({ "session": id }))
    }

    fn remember(&self, id: &str, s: Arc<Mutex<Session>>) {
        let mut all = self.sessions.lock().unwrap();
        if all.len() >= MAX_SESSIONS {
            let oldest = all
                .iter()
                .filter(|(_, s)| s.try_lock().is_ok_and(|s| !s.busy))
                .min_by_key(|(_, s)| s.lock().unwrap().last_used)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                all.remove(&k);
            }
        }
        all.insert(id.to_string(), s);
    }

    /// The chat's events from index `since` on, so the page can poll cheaply.
    pub fn events(&self, id: &str, since: usize) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("that chat has ended (Wardian restarted)")?;
        let s = s.lock().unwrap();
        let from = since.min(s.events.len());
        Ok(json!({ "events": s.events[from..], "next": s.events.len(), "busy": s.busy, "app": s.app }))
    }

    /// The chats of the last day, newest first, with what each needs: so any tab can show
    /// "working", pick up a chat, or run the browser test of a save nobody has tested yet.
    pub fn sessions(&self) -> Value {
        let now = unix_now();
        let mut out: Vec<(u64, Value)> = self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, s)| {
                let s = s.lock().unwrap();
                (now.saturating_sub(s.last_used) < 24 * 3600).then(|| (s.last_used, summary(id, &s, now)))
            })
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0));
        Value::Array(out.into_iter().map(|(_, v)| v).collect())
    }

    /// A tab asks to test save number `saved`. Yes only for the latest save, untested, and not
    /// being tested by another tab. Then every tab shows "Trying the app…".
    pub fn claim_test(&self, id: &str, saved: usize) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("that chat has ended (Wardian restarted)")?;
        let mut s = s.lock().unwrap();
        let now = unix_now();
        let ok = !s.busy
            && last_saved(&s.events) == Some(saved)
            && !tested(&s.events, saved)
            && s.test_claim.is_none_or(|(i, at)| i != saved || now.saturating_sub(at) > TEST_CLAIM_SECS);
        if ok {
            if s.test_claim.is_none_or(|(i, _)| i != saved) {
                s.events.push(json!({ "kind": "testing", "saved": saved, "text": "Trying the app in this browser…" }));
            }
            s.test_claim = Some((saved, now));
        }
        Ok(json!({ "claimed": ok }))
    }

    /// The result of a tab's browser test of save number `saved`.
    pub fn tested(&self, id: &str, body: &Value) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("that chat has ended (Wardian restarted)")?;
        let mut s = s.lock().unwrap();
        let saved = body["saved"].as_u64().ok_or("which save was tested?")? as usize;
        if tested(&s.events, saved) {
            return Ok(json!({ "recorded": false }));
        }
        let text: String = body["text"].as_str().unwrap_or("").chars().take(4000).collect();
        s.events.push(json!({ "kind": "tested", "saved": saved, "ok": body["ok"].as_bool() == Some(true), "text": text }));
        s.test_claim = None;
        s.last_used = unix_now();
        Ok(json!({ "recorded": true }))
    }

    pub fn stop(&self, id: &str) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("no such chat")?;
        s.lock().unwrap().cancel.store(true, Ordering::Relaxed);
        Ok(json!({ "stopping": true }))
    }
}

fn last_saved(events: &[Value]) -> Option<usize> {
    events.iter().rposition(|e| e["kind"] == "saved")
}

fn tested(events: &[Value], saved: usize) -> bool {
    events.iter().any(|e| e["kind"] == "tested" && e["saved"].as_u64() == Some(saved as u64))
}

/// One chat for the session list. `state`: working, untested (saved, nobody tested it yet),
/// testing, ready (the test passed), needs_you (an error, a stop, a failed test, or a question),
/// or idle (nothing asked yet).
fn summary(id: &str, s: &Session, now: u64) -> Value {
    let saved = last_saved(&s.events);
    let last = s.events.iter().rev().find(|e| e["kind"] != "tool");
    let claimed = s.test_claim.is_some_and(|(i, at)| Some(i) == saved && now.saturating_sub(at) <= TEST_CLAIM_SECS);
    // Claude often says a last word after it saves, so a save waits for its test whatever came after.
    let state = if s.busy {
        "working"
    } else if saved.is_some_and(|i| !tested(&s.events, i)) {
        if claimed { "testing" } else { "untested" }
    } else {
        match last.and_then(|e| e["kind"].as_str()) {
            None => "idle",
            Some("tested") if last.is_some_and(|e| e["ok"] == true) => "ready",
            // Claude's last word after a passed test still means "ready".
            Some("say") if s.events.iter().rev().find(|e| e["kind"] == "tested" || e["kind"] == "user" || e["kind"] == "test").is_some_and(|e| e["kind"] == "tested" && e["ok"] == true) => "ready",
            _ => "needs_you",
        }
    };
    // Automatic fixes since the user last wrote: the page stops after a few.
    let fixes = s.events.iter().rev().take_while(|e| e["kind"] != "user").filter(|e| e["kind"] == "test").count();
    let title: String = s.events.iter().find(|e| e["kind"] == "user").and_then(|e| e["text"].as_str()).unwrap_or("").chars().take(80).collect();
    json!({ "session": id, "app": s.app, "busy": s.busy, "state": state, "saved": saved, "fixes": fixes, "title": title, "last_used": s.last_used })
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn rand32() -> u32 {
    // Session ids only need to differ, not to be secret: every API call is admin-only.
    let mut b = [0u8; 4];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut b);
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    u32::from_le_bytes(b) ^ std::process::id().rotate_left(16) ^ nanos
}

fn is_app(dir: &Path) -> bool {
    APP_MARKERS.iter().any(|m| dir.join(m).is_file())
}

fn push_event(session: &Mutex<Session>, ev: Value) {
    session.lock().unwrap().events.push(ev);
}

/// The package Claude works on, in memory: path -> bytes.
type Files = BTreeMap<String, Vec<u8>>;

/// Reads an app folder, leaving out hidden files and build folders.
fn load(dir: &Path, prefix: &str, out: &mut Files) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        let rel = format!("{prefix}{name}");
        match e.file_type() {
            Ok(t) if t.is_dir() => load(&e.path(), &format!("{rel}/"), out),
            Ok(t) if t.is_file() => {
                if let Ok(b) = fs::read(e.path()) {
                    out.insert(rel, b);
                }
            }
            _ => {}
        }
    }
}

/// Writes the package to `dir`, adding the empty app.wasm a page app needs.
fn materialize(files: &Files, dir: &Path) -> Result<(), String> {
    for (rel, bytes) in files {
        let out = dir.join(rel);
        fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&out, bytes).map_err(|e| e.to_string())?;
    }
    if !files.contains_key("suite.json") && !files.contains_key("app.wasm") && files.contains_key("app.json") {
        fs::write(dir.join("app.wasm"), EMPTY_WASM).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A hidden folder inside the apps folder, so the final move is one rename.
fn staging_dir(root: &Path) -> PathBuf {
    root.join(format!(".ai-{}-{:08x}", unix_now(), rand32()))
}

fn check_files(files: &Files, root: &Path, name: &str) -> Result<(bool, String), String> {
    let dir = staging_dir(root);
    let r = materialize(files, &dir).map(|()| check_dir(&dir, name));
    let _ = fs::remove_dir_all(&dir);
    r.map_err(|e| format!("cannot write the files to check them: {e}"))
}

/// A free folder name for a new app: "notes", else "notes-2", "notes-3"…
fn free_name(root: &Path, wanted: &str) -> String {
    let base: String = wanted
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(40)
        .collect();
    let base = if safe_segment(&base) { base } else { "my-app".into() };
    let mut name = base.clone();
    let mut n = 2;
    while root.join(&name).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

/// Checks, then saves. The old version goes to the trash. Returns the app
/// name and the trash id of the old version, if there was one.
fn save(files: &Files, hub: &Hub, app: Option<&str>, wanted: &str) -> Result<(String, Option<String>), String> {
    let root = hub.local_root();
    let name = match app {
        Some(a) => a.to_string(),
        None => free_name(root, wanted),
    };
    let dir = staging_dir(root);
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    if let Err(e) = materialize(files, &dir) {
        let _ = fs::remove_dir_all(&dir);
        return Err(format!("cannot write the files: {e}"));
    }
    let (ok, report) = check_dir(&dir, &name);
    if !ok {
        let _ = fs::remove_dir_all(&dir);
        return Err(format!("not saved: the check found errors.\n{report}"));
    }
    let old = if root.join(&name).exists() { Some(hub.move_to_trash(&name)?) } else { None };
    if let Err(e) = fs::rename(&dir, root.join(&name)) {
        let _ = fs::remove_dir_all(&dir);
        return Err(format!("cannot save {name}: {e}"));
    }
    println!("ai: saved {name}");
    Ok((name, old))
}

fn size_text(n: usize) -> String {
    if n < 1024 { format!("{n} B") } else { format!("{:.1} KB", n as f64 / 1024.0) }
}

/// Runs one tool. Returns the text for Claude, whether it is an error, and
/// the line to show the user (if any).
fn run_tool(name: &str, input: &Value, files: &mut Files, hub: &Hub, session: &Mutex<Session>) -> (String, bool, Option<String>) {
    let path = input["path"].as_str().unwrap_or("").trim_start_matches("./");
    let bad_path = || (format!("\"{path}\" is not an allowed path: use letters, digits, '-', '_', '.' and '/'"), true, None);
    match name {
        "list_files" => {
            if files.is_empty() {
                return ("The package is empty.".into(), false, None);
            }
            let list = files.iter().map(|(p, b)| format!("{p}  {}", size_text(b.len()))).collect::<Vec<_>>().join("\n");
            (list, false, Some("looked at the files".into()))
        }
        "read_file" => match files.get(path) {
            None => (format!("no file \"{path}\""), true, None),
            Some(b) => match std::str::from_utf8(b) {
                Ok(t) => (t.to_string(), false, Some(format!("read {path}"))),
                Err(_) => (format!("{path} is binary ({}); it cannot be shown or changed", size_text(b.len())), false, None),
            },
        },
        "write_file" => {
            if !safe_rel(path) {
                return bad_path();
            }
            let content = input["content"].as_str().unwrap_or("");
            if path.ends_with(".wasm") {
                return ("You cannot write .wasm files. Do the work in JavaScript.".into(), true, None);
            }
            if !files.contains_key(path) && files.len() >= MAX_FILES {
                return (format!("a package made here may have at most {MAX_FILES} files"), true, None);
            }
            let append = input["append"].as_bool().unwrap_or(false);
            let entry = files.entry(path.to_string()).or_default();
            if !append {
                entry.clear();
            }
            if entry.len() + content.len() > MAX_FILE_CHARS {
                return (format!("{path} would be larger than 512 KB"), true, None);
            }
            entry.extend_from_slice(content.as_bytes());
            let verb = if append { "added to" } else { "wrote" };
            (format!("{verb} {path} ({})", size_text(entry.len())), false, Some(format!("{verb} {path} ({})", size_text(entry.len()))))
        }
        "delete_file" => match files.remove(path) {
            Some(_) => (format!("deleted {path}"), false, Some(format!("deleted {path}"))),
            None => (format!("no file \"{path}\""), true, None),
        },
        "add_components" => {
            let names: Vec<String> = input["components"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            let list = match crate::ui::files_for(&names) {
                Ok(l) => l,
                Err(e) => return (e, true, None),
            };
            let rels: Vec<String> = list.iter().map(|f| format!("ui/{f}")).collect();
            let mut out = Vec::new();
            for (f, rel) in list.iter().zip(&rels) {
                // Keep a copy the app already has: it may have been changed on purpose.
                if !files.contains_key(rel) {
                    files.insert(rel.clone(), crate::ui::file(f).unwrap_or("").as_bytes().to_vec());
                    out.push(format!("added {rel}"));
                }
            }
            if let Some(text) = files.get("suite.json").and_then(|b| String::from_utf8(b.clone()).ok()) {
                match crate::ui::wire_suite(&text, &rels) {
                    Ok((body, wired)) => {
                        if let Some(body) = body {
                            files.insert("suite.json".into(), body.into_bytes());
                        }
                        out.extend(wired.into_iter().map(|w| format!("suite.json {w}")));
                    }
                    Err(e) => out.push(format!("could not list them in suite.json ({e}); add them to \"styles\" and \"scripts\" yourself")),
                }
            } else {
                out.push(format!("Put these in the page's <head>, before your own CSS and scripts:\n{}", crate::ui::page_tags(&rels).join("\n")));
            }
            out.push(String::new());
            out.push(crate::ui::GUIDE.into());
            (out.join("\n"), false, Some(format!("added components: {}", names.join(", "))))
        }
        "check" => {
            let app = session.lock().unwrap().app.clone();
            match check_files(files, hub.local_root(), app.as_deref().unwrap_or("new-app")) {
                Ok((ok, report)) => {
                    let line = if ok { "check passed" } else { "check found problems; fixing them" };
                    (report, !ok, Some(line.into()))
                }
                Err(e) => (e, true, None),
            }
        }
        "finish" => {
            let app = session.lock().unwrap().app.clone();
            let wanted = input["name"].as_str().unwrap_or("my-app");
            match save(files, hub, app.as_deref(), wanted) {
                Ok((name, old)) => {
                    let summary = input["summary"].as_str().unwrap_or("").to_string();
                    let mut s = session.lock().unwrap();
                    s.app = Some(name.clone());
                    s.events.push(json!({ "kind": "saved", "app": name, "replaced": old, "text": summary }));
                    (format!("Saved as \"{name}\". The user can open it now."), false, None)
                }
                Err(e) => (e, true, Some("not saved yet: the check found problems; fixing them".into())),
            }
        }
        _ => (format!("unknown tool {name}"), true, None),
    }
}

/// One turn: Claude works until it stops calling tools, with a limit on steps.
fn run_turn(api: &Api, hub: &Hub, session: &Mutex<Session>, cancel: &AtomicBool) -> Result<(), String> {
    let (mut messages, app) = {
        let s = session.lock().unwrap();
        (s.messages.clone(), s.app.clone())
    };
    // Start from the app as it is on disk now, so edits made by hand are kept.
    let mut files = Files::new();
    if let Some(app) = &app {
        load(&hub.local_root().join(app), "", &mut files);
        files.retain(|p, b| !(p == "app.wasm" && b.as_slice() == EMPTY_WASM));
    }
    let start_len = messages.len();
    let mut saved_since_change = true;
    let mut nudged = false;
    let result = (|| {
        for _ in 0..MAX_STEPS {
            if cancel.load(Ordering::Relaxed) {
                return Err("stopped".to_string());
            }
            let reply = api.messages(&messages, cancel)?;
            // An empty text block is refused when sent back, so drop any.
            let content: Vec<Value> = reply["content"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|b| !(b["type"] == "text" && b["text"].as_str().is_none_or(|t| t.trim().is_empty())))
                .collect();
            if content.is_empty() {
                return Ok(());
            }
            for block in &content {
                if block["type"] == "text" {
                    let t = block["text"].as_str().unwrap_or("").trim();
                    if !t.is_empty() {
                        push_event(session, json!({ "kind": "say", "text": t }));
                    }
                }
            }
            messages.push(json!({ "role": "assistant", "content": content }));
            let calls: Vec<&Value> = content.iter().filter(|b| b["type"] == "tool_use").collect();
            let cut_off = reply["stop_reason"] == "max_tokens";
            if calls.is_empty() {
                if cut_off {
                    messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": "Your reply was cut off at the length limit. Continue, and write large files in parts with append." }] }));
                    continue;
                }
                if !saved_since_change && !nudged {
                    nudged = true;
                    messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": "You changed files but did not call finish, so nothing is saved. Call finish now, or say what stops you." }] }));
                    continue;
                }
                return Ok(());
            }
            let mut results = Vec::new();
            for call in calls {
                let name = call["name"].as_str().unwrap_or("");
                let (text, is_error, shown) = if cut_off && name != "finish" {
                    ("Your reply was cut off at the length limit, so this call was not run. Write large files in smaller parts with append.".to_string(), true, None)
                } else {
                    run_tool(name, &call["input"], &mut files, hub, session)
                };
                match name {
                    "write_file" | "delete_file" if !is_error => saved_since_change = false,
                    "finish" if !is_error => saved_since_change = true,
                    _ => {}
                }
                if let Some(line) = shown {
                    push_event(session, json!({ "kind": "tool", "text": line }));
                }
                results.push(json!({ "type": "tool_result", "tool_use_id": call["id"], "content": text, "is_error": is_error }));
            }
            messages.push(json!({ "role": "user", "content": results }));
        }
        Err(format!("Claude used all {MAX_STEPS} steps for this message. Send another message to continue."))
    })();
    // Keep the conversation only up to a complete exchange, so the next
    // message always follows a valid history.
    let mut s = session.lock().unwrap();
    if result.is_ok() || messages.last().is_some_and(|m| m["role"] == "user" && m["content"][0]["type"] == "tool_result") {
        s.messages = messages;
    } else if messages.len() > start_len {
        // Stopped or failed mid-request: drop the unanswered last step, keep the rest.
        while messages.len() > start_len && messages.last().is_some_and(|m| m["role"] == "assistant") {
            messages.pop();
        }
        s.messages = messages;
    }
    result
}

#[cfg(test)]
mod sample_tests {
    use super::json_in;
    use serde_json::json;

    #[test]
    fn finds_json_in_an_answer() {
        assert_eq!(json_in("```json\n{\"a\": 1}\n```"), Some(json!({"a": 1})));
        assert_eq!(json_in("Here: [1, 2]"), Some(json!([1, 2])));
        assert_eq!(json_in("no json"), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_names_and_page_marker() {
        let root = std::env::temp_dir().join(format!("wardian-ai-test-{}", std::process::id()));
        fs::create_dir_all(root.join("notes")).unwrap();
        assert_eq!(free_name(&root, "Notes"), "notes-2");
        assert_eq!(free_name(&root, "My Budget!"), "my-budget");
        assert_eq!(free_name(&root, "../.."), "my-app");

        let mut files = Files::new();
        files.insert("app.json".into(), br#"{"format":1,"title":"Hi","page":"index.html"}"#.to_vec());
        files.insert("index.html".into(), b"<!doctype html><p>hi</p>".to_vec());
        let (ok, report) = check_files(&files, &root, "hi").unwrap();
        assert!(ok, "{report}");
        files.remove("index.html");
        assert!(!check_files(&files, &root, "hi").unwrap().0, "a missing page must fail");
        let _ = fs::remove_dir_all(root);
    }
}
