//! Make an app's workshop as WebAssembly (ADR-2610111500): Wardian's own source, included by path,
//! with one entry point. The hosted builder (wardian-web) calls it from Node.
//!
//! The interface is bytes of JSON, so it needs no binding generator:
//!   alloc(len) -> ptr        the caller writes a request there
//!   call(ptr, len) -> u64    runs it; the reply is at (result >> 32), (result & 0xffffffff) bytes long
//!   dealloc(ptr, len)        frees a request or a reply
//!
//! Requests: {"op":"tools"} · {"op":"system"} ·
//!   {"op":"run","name":…,"input":{…},"files":{"path":"text",…},"app":null|"name"} ·
//!   {"op":"package","name":…,"files":{…}} · {"op":"preview","name":…,"files":{…}} ·
//!   {"op":"skillsRequest","input":{…},"files":{…}} · {"op":"skillsOutcome","reply":{…}}
//! The reply to "run": {"text","isError","line"?,"files":{…},"saved"?:{"name","summary"}}. The
//! reply to "package" is not JSON: it is the `.wardian` file's bytes. The reply to "preview" is
//! {"files":{"path":"text",…},"entry":"run/<name>/"|"apps/<name>/<page>"}, or {"error"}: the package
//! as a static site that runs with no Wardian behind it, under any root (demos.rs).
//! The skills agent (ADR-2610131000) is a model call the host makes: "skillsRequest" gives its
//! {"system","messages","maxTokens"} (the host adds the model), or {"error"} for a bad brief, which is
//! the tool's error; "skillsOutcome" turns the model's reply into {"text","isError","line"?}.
//! Goldfish reviews (ADR-2610281000, ADR-2610281200) are model calls too, one per lens:
//! {"op":"reviewRequest","input":{stage,asked?},"files":{…}} gives {"requests":[{"lens","system",
//! "messages"}…],"maxTokens"} or {"error"}, and {"op":"reviewOutcome","input":{…},"replies":[{"lens",
//! "reply"}|{"lens","error"}…]} gives {"text","isError","line"?}.
//! The Elephant-Goldfish gates (ADR-2610281200): {"op":"flow","flow":{…}|null,"event":…} with event
//! "start" (and "newApp"), "said" (the person answered), "refuse" (and "tool","input": may it run?)
//! or "ran" (and "tool","input","isError") gives {"flow","waiting","refuse"?}.
//! An AI app's skill and tools (ADR-2610131300): {"op":"aiTools","files":{…}} gives what App settings
//! shows; {"op":"aiToolsApply","files":{…},"repository":{…}} gives {"changes":{"path":"text",…}}, the
//! ai app's skill.md and tools.json to write (the caller saves through finish, which checks), or
//! {"error"}; {"op":"aiImproveRequest","files":{…},"repository":{…},"wanted":…} gives Improve with
//! AI's {"system","messages","maxTokens"} (the host adds the model) or {"error"}; and
//! {"op":"aiImproveOutcome","reply":{…},"files":{…}} turns the model's reply into
//! {"repository","notes","proposed"} or {"error"}.
//! An app's `claude:sample` in a hosted preview (ADR-2610141500): {"op":"sampleRequest","body":{prompt,
//! json, tier}} gives {"messages","quick","json"} (the host adds the model and the output limit) or
//! {"error"} with the app's error code; {"op":"sampleOutcome","reply":{…},"json":bool} turns the
//! model's reply into what the app gets, {"text","truncated"} or {"value"}, or {"error"}.
#![allow(dead_code)]

#[path = "../../../src/domain"]
mod domain {
    pub mod ai_tools;
    pub mod check;
    pub mod components;
    pub mod export;
    pub mod grants;
    pub mod history;
    pub mod import_plan;
    pub mod package;
    pub mod studio;
    pub mod suite;
}
#[path = "../../../src/ports"]
mod ports {
    pub mod assets;
}
#[path = "../../../src/usecases"]
mod usecases {
    pub mod ai_tools;
    pub mod demos;
    pub mod workshop;
}
#[path = "../../../src/adapters/secondary"]
mod secondary {
    pub mod embedded_assets;
}

use secondary::embedded_assets::Embedded;
use serde_json::{json, Map, Value};
use usecases::workshop::{self, Files};

/// One request, as JSON in; JSON out, or a `.wardian` file's bytes for "package".
pub fn handle(request: &str) -> Vec<u8> {
    let req: Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("the request is not JSON: {e}") }).to_string().into_bytes(),
    };
    let reply = match req["op"].as_str() {
        Some("tools") => workshop::tools(),
        Some("system") => workshop::system(&Embedded),
        Some("run") => run(&req),
        Some("package") => return workshop::package(req["name"].as_str().unwrap_or("app"), &files_of(&req), env!("WARDIAN_VERSION")),
        Some("preview") => preview(&req),
        Some("skillsRequest") => match workshop::skills_request(&Embedded, &req["input"], &files_of(&req)) {
            Ok(mut r) => {
                r["maxTokens"] = json!(workshop::SKILLS_MAX_TOKENS);
                r
            }
            Err(e) => json!({ "error": e }),
        },
        Some("skillsOutcome") => {
            let out = workshop::skills_outcome(&req["reply"]);
            let mut reply = json!({ "text": out.text, "isError": out.is_error });
            if let Some(line) = out.shown {
                reply["line"] = json!(line);
            }
            reply
        }
        Some("reviewRequest") => match workshop::review_requests(&Embedded, &req["input"], &files_of(&req)) {
            Ok(r) => json!({
                "requests": r.into_iter().map(|(lens, body)| json!({ "lens": lens, "system": body["system"], "messages": body["messages"] })).collect::<Vec<_>>(),
                "maxTokens": workshop::REVIEW_MAX_TOKENS,
            }),
            Err(e) => json!({ "error": e }),
        },
        Some("reviewOutcome") => {
            let replies: Vec<(String, Result<Value, String>)> = req["replies"]
                .as_array()
                .map(|a| a.iter().map(|r| (r["lens"].as_str().unwrap_or("").to_string(), r.get("error").and_then(Value::as_str).map_or_else(|| Ok(r["reply"].clone()), |e| Err(e.to_string())))).collect())
                .unwrap_or_default();
            let out = workshop::review_outcome(&req["input"], &replies);
            let mut reply = json!({ "text": out.text, "isError": out.is_error });
            if let Some(line) = out.shown {
                reply["line"] = json!(line);
            }
            reply
        }
        Some("flow") => flow(&req),
        Some("aiTools") => usecases::ai_tools::view(&files_of(&req)),
        Some("aiToolsApply") => {
            let before = files_of(&req);
            let mut after = before.clone();
            match usecases::ai_tools::apply(&mut after, &req["repository"]) {
                Ok(()) => {
                    let changes: Map<String, Value> = after
                        .into_iter()
                        .filter(|(k, v)| before.get(k) != Some(v))
                        .map(|(k, v)| (k, Value::String(String::from_utf8_lossy(&v).into_owned())))
                        .collect();
                    json!({ "changes": changes })
                }
                Err(e) => json!({ "error": e }),
            }
        }
        Some("aiImproveRequest") => match usecases::ai_tools::improve_request_for(&files_of(&req), &req["repository"], req["wanted"].as_str().unwrap_or("")) {
            Ok(r) => json!({ "system": r["system"], "messages": r["messages"], "maxTokens": r["max_tokens"] }),
            Err(e) => json!({ "error": e }),
        },
        Some("aiImproveOutcome") => usecases::ai_tools::improve_outcome_for(&req["reply"], &files_of(&req)).unwrap_or_else(|e| json!({ "error": e })),
        Some("sampleRequest") => match domain::studio::sample_ask(&req["body"]) {
            Ok((ask, quick, json)) => json!({ "messages": [{ "role": "user", "content": ask }], "quick": quick, "json": json }),
            Err(e) => json!({ "error": e }),
        },
        Some("sampleOutcome") => domain::studio::sample_answer(&req["reply"], req["json"].as_bool().unwrap_or(false)).unwrap_or_else(|e| json!({ "error": e })),
        _ => json!({ "error": "op must be tools, system, run, package, preview, skillsRequest, skillsOutcome, reviewRequest, reviewOutcome, flow, aiTools, aiToolsApply, aiImproveRequest, aiImproveOutcome, sampleRequest or sampleOutcome" }),
    };
    reply.to_string().into_bytes()
}

fn files_of(req: &Value) -> Files {
    req["files"]
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").as_bytes().to_vec())).collect())
        .unwrap_or_default()
}

fn preview(req: &Value) -> Value {
    let files: Vec<(String, Vec<u8>)> = files_of(req).into_iter().collect();
    match usecases::demos::preview_site(&Embedded, req["name"].as_str().unwrap_or(""), &files) {
        Ok((site, entry)) => {
            let site: Map<String, Value> = site.into_iter().map(|(k, v)| (k, Value::String(String::from_utf8_lossy(&v).into_owned()))).collect();
            json!({ "files": site, "entry": entry })
        }
        Err(e) => json!({ "error": e }),
    }
}

fn run(req: &Value) -> Value {
    let mut files = files_of(req);
    let name = req["name"].as_str().unwrap_or("");
    let out = workshop::run(&Embedded, name, &req["input"], &mut files, req["app"].as_str());
    let files: Map<String, Value> = files.into_iter().map(|(k, v)| (k, Value::String(String::from_utf8_lossy(&v).into_owned()))).collect();
    let mut reply = json!({ "text": out.text, "isError": out.is_error, "files": files });
    if let Some(line) = out.shown {
        reply["line"] = json!(line);
    }
    if let Some(saved) = out.saved {
        reply["saved"] = json!({ "name": saved.name, "summary": saved.summary });
    }
    reply
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` and `len` come from `alloc`, or from a reply of `call`, and are freed once.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len));
}

/// # Safety
/// `ptr` holds `len` bytes of UTF-8 the caller wrote after `alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn call(ptr: *const u8, len: usize) -> u64 {
    let request = String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned();
    let mut reply = handle(&request);
    reply.shrink_to_fit();
    let (out, n) = (reply.as_mut_ptr(), reply.len());
    std::mem::forget(reply);
    ((out as u64) << 32) | n as u64
}

/// One step of the Elephant-Goldfish gates (ADR-2610281200) on the caller's saved flow.
fn flow(req: &Value) -> Value {
    use domain::studio::Flow;
    let mut f: Flow = serde_json::from_value(req["flow"].clone()).unwrap_or_else(|_| Flow::start(req["newApp"].as_bool().unwrap_or(false)));
    let tool = req["tool"].as_str().unwrap_or("");
    let mut refuse = None;
    match req["event"].as_str() {
        Some("start") => f = Flow::start(req["newApp"].as_bool().unwrap_or(false)),
        Some("said") => f.person_said(),
        Some("refuse") => refuse = f.refuse(tool, &req["input"], workshop::REVIEW_TOOL),
        Some("ran") => f.ran(tool, &req["input"], workshop::REVIEW_TOOL, req["isError"].as_bool().unwrap_or(false)),
        _ => return json!({ "error": "event must be start, said, refuse or ran" }),
    }
    let mut out = json!({ "flow": f, "waiting": f.waiting() });
    if let Some(why) = refuse {
        out["refuse"] = json!(why);
    }
    out
}
