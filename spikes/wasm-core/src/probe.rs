//! Probes the browser calls, each a real Wardian function on in-memory input. A probe that
//! returns ran; one that panics traps (`unreachable`), which is the finding.

use crate::domain::{check, package, suite};

/// `wardian check` on a minimal module app, held in memory. Returns the error count.
#[no_mangle]
pub extern "C" fn probe_check() -> u32 {
    let wasm: &[u8] = b"\0asm\x01\0\0\0";
    let json: &[u8] = br#"{"format":1,"title":"Adder"}"#;
    let files = vec![("app.wasm".to_string(), wasm.len() as u64), ("app.json".to_string(), json.len() as u64)];
    let read = |p: &str| match p {
        "app.wasm" => Some(wasm.to_vec()),
        "app.json" => Some(json.to_vec()),
        _ => None,
    };
    let mut r = check::Report::default();
    check::check_package("adder", &files, &read, &mut r);
    r.errors.len() as u32
}

/// A suite part's frame document, built the way the server builds it. Returns its length.
#[no_mangle]
pub extern "C" fn probe_frame() -> u32 {
    let read = |p: &str| -> Option<Vec<u8>> {
        match p {
            "suite.json" => Some(br#"{"format":1,"title":"S","apps":[{"name":"a","slot":"main"}]}"#.to_vec()),
            "apps/a/view.html" => Some(b"<p>hi</p>".to_vec()),
            "apps/a/app.js" => Some(b"Kernel.register('a',{});".to_vec()),
            _ => None,
        }
    };
    suite::frame(&read, "/* shim */", Some("a")).map(|h| h.len() as u32).unwrap_or(0)
}

/// The clock, through `package::unix_now`.
#[no_mangle]
pub extern "C" fn probe_now() -> u64 {
    package::unix_now()
}

/// A thread, as `usecases::jobs` starts one per background job.
#[no_mangle]
pub extern "C" fn probe_thread() -> u32 {
    std::thread::spawn(|| 7u32).join().unwrap_or(0)
}
