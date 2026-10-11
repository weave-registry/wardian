//! Make an app's workshop as WebAssembly (ADR-2610111500): Wardian's own source, included by path,
//! with one entry point. The hosted builder (wardian-web) calls it from Node.
//!
//! The interface is bytes of JSON, so it needs no binding generator:
//!   alloc(len) -> ptr        the caller writes a request there
//!   call(ptr, len) -> u64    runs it; the reply is at (result >> 32), (result & 0xffffffff) bytes long
//!   dealloc(ptr, len)        frees a request or a reply
//!
//! Requests: {"op":"tools"} · {"op":"system"} ·
//!   {"op":"run","name":…,"input":{…},"files":{"path":"text",…},"app":null|"name"}
//! The reply to "run": {"text","isError","line"?,"files":{…},"saved"?:{"name","summary"}}.
#![allow(dead_code)]

#[path = "../../../src/domain"]
mod domain {
    pub mod check;
    pub mod components;
    pub mod grants;
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
    pub mod workshop;
}
#[path = "../../../src/adapters/secondary"]
mod secondary {
    pub mod embedded_assets;
}

use secondary::embedded_assets::Embedded;
use serde_json::{json, Map, Value};
use usecases::workshop::{self, Files};

/// One request, as JSON in and JSON out.
pub fn handle(request: &str) -> String {
    let req: Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(e) => return json!({ "error": format!("the request is not JSON: {e}") }).to_string(),
    };
    let reply = match req["op"].as_str() {
        Some("tools") => workshop::tools(),
        Some("system") => workshop::system(&Embedded),
        Some("run") => run(&req),
        _ => json!({ "error": "op must be tools, system or run" }),
    };
    reply.to_string()
}

fn run(req: &Value) -> Value {
    let mut files: Files = req["files"]
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").as_bytes().to_vec())).collect())
        .unwrap_or_default();
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
    let mut reply = handle(&request).into_bytes();
    reply.shrink_to_fit();
    let (out, n) = (reply.as_mut_ptr(), reply.len());
    std::mem::forget(reply);
    ((out as u64) << 32) | n as u64
}
