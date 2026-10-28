//! "Make an app", as data: one chat with Claude, the events the page shows, and what each chat
//! needs next. The model calls and the files go through ports; this module only decides.

use super::package::safe_segment;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{atomic::AtomicBool, Arc};

/// How long a tab may hold a test before another tab may take it over (the tab may have closed).
const TEST_CLAIM_SECS: u64 = 90;
/// The smallest valid WebAssembly module: the marker for a page app with no
/// WebAssembly of its own.
pub const EMPTY_WASM: &[u8] = b"\0asm\x01\0\0\0";

/// Make an app's Elephant-Goldfish gates (ADR-2610281200; EGM, egm.dev, by Google's EGM team).
/// The builder (the elephant) writes the plan; goldfish reviewers check it; the person approves it
/// before any code is written; goldfish check the build before it is saved. Both builders keep one
/// of these per chat and ask it before every tool runs, so the method holds whatever the model does.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flow {
    pub stage: Stage,
    /// A build review ran since the person's last message. The fixes after it are its resolution,
    /// not new work, so they need no other review: reviewing until nothing is found never ends.
    pub build_reviewed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// Only the plan may be written: README.md, suite.json, app.json. A design review moves on.
    Plan,
    /// The plan was reviewed; it waits for the person's answer, which moves on to Build.
    Approval,
    /// Code may be written; finish needs a build review since the person's last message.
    Build,
}

/// The files a plan is made of, which may be written before the person approves it.
const PLAN_FILES: [&str; 3] = ["README.md", "suite.json", "app.json"];

impl Flow {
    /// A new app starts at the plan. A change to an app starts building: the builder's prompt asks
    /// it to plan and ask first when the change is large, and finish still needs a build review.
    pub fn start(new_app: bool) -> Flow {
        Flow { stage: if new_app { Stage::Plan } else { Stage::Build }, build_reviewed: false }
    }

    /// The person answered. A reviewed plan is approved; their answer may also ask for changes,
    /// which the builder makes to the plan before it writes code. What they ask for next is new
    /// work, so its finish needs a build review of its own.
    pub fn person_said(&mut self) {
        if self.stage == Stage::Approval {
            self.stage = Stage::Build;
        }
        self.build_reviewed = false;
    }

    /// Why a tool may not run now, or None. `review` is the tool that dispatches goldfish reviews.
    pub fn refuse(&self, tool: &str, input: &Value, review: &str) -> Option<String> {
        let path = input["path"].as_str().unwrap_or("").trim_start_matches("./");
        let code = matches!(tool, "write_file" | "delete_file" | "add_components") && !(tool != "add_components" && PLAN_FILES.contains(&path));
        match self.stage {
            Stage::Plan if code => Some(format!("Not yet: plan first. Write README.md (the blueprint and Layout), then call {review} with stage \"design\". No code until the plan is reviewed and the user approves it.")),
            Stage::Approval if code => Some("Not yet: the user has not approved the plan. End your turn now with the approval message: what you will build, what the reviewers changed, and each open decision as a question with the option you recommend; ask them to reply \"go\" or say what to change.".into()),
            Stage::Plan | Stage::Approval if tool == "finish" => Some("Not yet: nothing is built. The plan must be reviewed and approved first.".into()),
            Stage::Build if tool == "finish" && !self.build_reviewed => Some(format!("Not yet: call {review} with stage \"build\" first, so reviewers who have not seen this chat check the files against README.md; fix what they find, then finish (once: no second review).")),
            _ => None,
        }
    }

    /// After a tool ran. A design review, even a failed one, puts the plan to the person; a build
    /// review, even a failed one, lets finish save.
    pub fn ran(&mut self, tool: &str, input: &Value, review: &str, _is_error: bool) {
        if tool == review {
            match input["stage"].as_str() {
                Some("design") if self.stage == Stage::Plan => self.stage = Stage::Approval,
                Some("build") if self.stage == Stage::Build => self.build_reviewed = true,
                _ => {}
            }
        }
    }

    /// The turn may end without a save: the plan waits for the person's answer.
    pub fn waiting(&self) -> bool {
        self.stage != Stage::Build
    }
}

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
    /// The Elephant-Goldfish gates (ADR-2610281200).
    pub flow: Flow,
    /// Files written but not yet saved, kept between turns: a new app's plan waits for the
    /// person's approval before anything is saved. None once saved.
    pub draft: Option<BTreeMap<String, Vec<u8>>>,
}

impl Session {
    pub fn new(app: Option<String>, now: u64) -> Session {
        let flow = Flow::start(app.is_none());
        Session { app, messages: Vec::new(), events: Vec::new(), busy: false, cancel: Arc::new(AtomicBool::new(false)), last_used: now, test_claim: None, flow, draft: None }
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

/// The most an app may send in one `claude:sample` prompt.
pub const MAX_SAMPLE_CHARS: usize = 60_000;

/// One `claude:sample` request from an app, checked: the prompt to send Claude, whether the quick
/// model answers, and whether the answer must be JSON. `body`: {prompt, json, tier}. Errors start
/// with the code the app gets. Wardian's server and the hosted builder (workshop.wasm) both use it.
pub fn sample_ask(body: &Value) -> Result<(String, bool, bool), String> {
    let prompt = body["prompt"].as_str().unwrap_or("").trim();
    if prompt.is_empty() {
        return Err("error: the prompt is empty".into());
    }
    if prompt.chars().count() > MAX_SAMPLE_CHARS {
        return Err("prompt_too_large: the prompt is too long".into());
    }
    let quick = body["tier"].as_str() == Some("quick");
    let json = body["json"].as_bool().unwrap_or(false);
    let ask = if json { format!("{prompt}\n\nAnswer with the JSON only, no other words.") } else { prompt.to_string() };
    Ok((ask, quick, json))
}

/// Claude's reply (Messages format) to a `claude:sample` request, as the app gets it:
/// {text, truncated}, or {value} when it asked for JSON.
pub fn sample_answer(reply: &Value, json: bool) -> Result<Value, String> {
    let text: String = reply["content"]
        .as_array()
        .map(|blocks| blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(""))
        .unwrap_or_default();
    let stop = reply["stop_reason"].as_str().unwrap_or("");
    if stop == "refusal" {
        return Err("refused: Claude declined this request".into());
    }
    if json {
        let value = json_in(&text).ok_or("invalid_json: the answer was not JSON")?;
        return Ok(json!({ "value": value }));
    }
    Ok(json!({ "text": text, "truncated": stop == "max_tokens" }))
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
    use super::{free_name, json_in, sample_answer, sample_ask, MAX_SAMPLE_CHARS};
    use serde_json::json;

    #[test]
    fn studio_a_sample_request_is_checked_and_asks_for_json_when_wanted() {
        assert_eq!(sample_ask(&json!({"prompt": "  hi  "})).unwrap(), ("hi".to_string(), false, false));
        let (ask, quick, j) = sample_ask(&json!({"prompt": "list", "json": true, "tier": "quick"})).unwrap();
        assert!(ask.starts_with("list") && ask.contains("JSON only") && quick && j);
        assert!(sample_ask(&json!({"prompt": " "})).unwrap_err().starts_with("error: "));
        assert!(sample_ask(&json!({"prompt": "x".repeat(MAX_SAMPLE_CHARS + 1)})).unwrap_err().starts_with("prompt_too_large: "));
    }

    #[test]
    fn studio_a_sample_answer_is_text_json_or_a_coded_error() {
        let reply = |text: &str, stop: &str| json!({"content": [{"type": "text", "text": text}], "stop_reason": stop});
        assert_eq!(sample_answer(&reply("hello", "end_turn"), false).unwrap(), json!({"text": "hello", "truncated": false}));
        assert_eq!(sample_answer(&reply("hel", "max_tokens"), false).unwrap()["truncated"], true);
        assert_eq!(sample_answer(&reply("Here: {\"a\": 1}", "end_turn"), true).unwrap(), json!({"value": {"a": 1}}));
        assert!(sample_answer(&reply("no", "end_turn"), true).unwrap_err().starts_with("invalid_json: "));
        assert!(sample_answer(&reply("", "refusal"), false).unwrap_err().starts_with("refused: "));
    }

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
