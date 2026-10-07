//! "Make an app", as data: one chat with Claude, the events the page shows, and what each chat
//! needs next. The model calls and the files go through ports; this module only decides.

use super::package::safe_segment;
use serde_json::{json, Value};
use std::sync::{atomic::AtomicBool, Arc};

/// How long a tab may hold a test before another tab may take it over (the tab may have closed).
const TEST_CLAIM_SECS: u64 = 90;
/// The smallest valid WebAssembly module: the marker for a page app with no
/// WebAssembly of its own.
pub const EMPTY_WASM: &[u8] = b"\0asm\x01\0\0\0";

/// One chat. It lives in memory only; the apps it saves are ordinary folders.
pub struct Session {
    /// The app this chat creates or changes; None until the first save of a new app.
    pub app: Option<String>,
    pub messages: Vec<Value>,
    /// What the user sees: their messages, Claude's words, each tool step.
    pub events: Vec<Value>,
    pub busy: bool,
    pub cancel: Arc<AtomicBool>,
    pub last_used: u64,
    /// A browser tab that is testing the latest save: (index of the saved event, when it claimed it).
    /// Any tab may run the test, so the chat goes on while you work elsewhere; the claim keeps two
    /// tabs from testing the same save. A claim older than TEST_CLAIM_SECS is given up.
    pub test_claim: Option<(usize, u64)>,
}

impl Session {
    pub fn new(app: Option<String>, now: u64) -> Session {
        Session { app, messages: Vec::new(), events: Vec::new(), busy: false, cancel: Arc::new(AtomicBool::new(false)), last_used: now, test_claim: None }
    }

    /// A tab asks to test save number `saved`. Yes only for the latest save, untested, and not
    /// being tested by another tab. Then every tab shows "Trying the app…".
    pub fn claim_test(&mut self, saved: usize, now: u64) -> bool {
        let ok = !self.busy
            && last_saved(&self.events) == Some(saved)
            && !tested(&self.events, saved)
            && self.test_claim.is_none_or(|(i, at)| i != saved || now.saturating_sub(at) > TEST_CLAIM_SECS);
        if ok {
            if self.test_claim.is_none_or(|(i, _)| i != saved) {
                self.events.push(json!({ "kind": "testing", "saved": saved, "text": "Trying the app in this browser…" }));
            }
            self.test_claim = Some((saved, now));
        }
        ok
    }

    /// Records a tab's browser test of save number `saved`. False when it was already recorded.
    pub fn record_test(&mut self, saved: usize, ok: bool, text: &str, now: u64) -> bool {
        if tested(&self.events, saved) {
            return false;
        }
        let text: String = text.chars().take(4000).collect();
        self.events.push(json!({ "kind": "tested", "saved": saved, "ok": ok, "text": text }));
        self.test_claim = None;
        self.last_used = now;
        true
    }

    /// One chat for the session list. `state`: working, untested (saved, nobody tested it yet),
    /// testing, ready (the test passed), needs_you (an error, a stop, a failed test, or a question),
    /// or idle (nothing asked yet).
    pub fn summary(&self, id: &str, now: u64) -> Value {
        let saved = last_saved(&self.events);
        let last = self.events.iter().rev().find(|e| e["kind"] != "tool");
        let claimed = self.test_claim.is_some_and(|(i, at)| Some(i) == saved && now.saturating_sub(at) <= TEST_CLAIM_SECS);
        // Claude often says a last word after it saves, so a save waits for its test whatever came after.
        let state = if self.busy {
            "working"
        } else if saved.is_some_and(|i| !tested(&self.events, i)) {
            if claimed { "testing" } else { "untested" }
        } else {
            match last.and_then(|e| e["kind"].as_str()) {
                None => "idle",
                Some("tested") if last.is_some_and(|e| e["ok"] == true) => "ready",
                // Claude's last word after a passed test still means "ready".
                Some("say") if self.events.iter().rev().find(|e| e["kind"] == "tested" || e["kind"] == "user" || e["kind"] == "test").is_some_and(|e| e["kind"] == "tested" && e["ok"] == true) => "ready",
                _ => "needs_you",
            }
        };
        // Automatic fixes since the user last wrote: the page stops after a few.
        let fixes = self.events.iter().rev().take_while(|e| e["kind"] != "user").filter(|e| e["kind"] == "test").count();
        let title: String = self.events.iter().find(|e| e["kind"] == "user").and_then(|e| e["text"].as_str()).unwrap_or("").chars().take(80).collect();
        json!({ "session": id, "app": self.app, "busy": self.busy, "state": state, "saved": saved, "fixes": fixes, "title": title, "last_used": self.last_used })
    }
}

fn last_saved(events: &[Value]) -> Option<usize> {
    events.iter().rposition(|e| e["kind"] == "saved")
}

fn tested(events: &[Value], saved: usize) -> bool {
    events.iter().any(|e| e["kind"] == "tested" && e["saved"].as_u64() == Some(saved as u64))
}

/// The first JSON object or array in a model's answer, which may wrap it in
/// a code fence or a sentence.
pub fn json_in(text: &str) -> Option<Value> {
    let start = text.find(['{', '['])?;
    let end = text.rfind(['}', ']'])?;
    (end > start).then(|| serde_json::from_str(&text[start..=end]).ok()).flatten()
}

/// A free folder name for a new app: "notes", else "notes-2", "notes-3"… `taken` says whether
/// a name is in use.
pub fn free_name(wanted: &str, taken: &dyn Fn(&str) -> bool) -> String {
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
    while taken(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

pub fn size_text(n: usize) -> String {
    if n < 1024 { format!("{n} B") } else { format!("{:.1} KB", n as f64 / 1024.0) }
}

#[cfg(test)]
mod tests {
    use super::{free_name, json_in};
    use serde_json::json;

    #[test]
    fn finds_json_in_an_answer() {
        assert_eq!(json_in("```json\n{\"a\": 1}\n```"), Some(json!({"a": 1})));
        assert_eq!(json_in("Here: [1, 2]"), Some(json!([1, 2])));
        assert_eq!(json_in("no json"), None);
    }

    #[test]
    fn free_names() {
        let taken = |n: &str| n == "notes";
        assert_eq!(free_name("Notes", &taken), "notes-2");
        assert_eq!(free_name("My Budget!", &taken), "my-budget");
        assert_eq!(free_name("../..", &taken), "my-app");
    }
}
