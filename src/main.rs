mod source;

use source::{safe_segment, Drive, Source};
use std::{path::{Path, PathBuf}, sync::Arc, thread, time::Duration};
use tiny_http::{Header, Method, Request, Response, Server};

const INDEX_HTML: &str = include_str!("../static/index.html");

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("wasm") => "application/wasm", // required for instantiateStreaming
        Some("json") => "application/json",
        Some("js") => "text/javascript",
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

fn handle(req: Request, src: &Source) {
    if *req.method() != Method::Get {
        let _ = req.respond(Response::empty(405));
        return;
    }
    let url = req.url().split('?').next().unwrap_or("/").to_string();
    let parts: Vec<&str> = url.trim_matches('/').split('/').collect();

    let result = match parts.as_slice() {
        [""] => req.respond(
            Response::from_string(INDEX_HTML)
                .with_header(header("Content-Type", mime(Path::new("x.html")))),
        ),
        ["api", "apps"] => {
            let items: Vec<String> = src.list_apps().iter().map(|n| format!("\"{n}\"")).collect();
            req.respond(
                Response::from_string(format!("[{}]", items.join(",")))
                    .with_header(header("Content-Type", "application/json")),
            )
        }
        ["apps", name, file] if safe_segment(name) && safe_segment(file) => {
            match src.read(name, file) {
                Some(bytes) => req.respond(
                    Response::from_data(bytes)
                        .with_header(header("Content-Type", mime(Path::new(file))))
                        .with_header(header("Cache-Control", "no-cache")),
                ),
                None => req.respond(Response::from_string("not found").with_status_code(404)),
            }
        }
        _ => req.respond(Response::from_string("not found").with_status_code(404)),
    };
    if let Err(e) = result {
        eprintln!("response error: {e}");
    }
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn main() {
    // GDRIVE_FOLDER_ID set -> serve from Drive; otherwise from a local dir.
    let source = match env("GDRIVE_FOLDER_ID") {
        Some(folder) => {
            let key = env("GDRIVE_SA_KEY").expect("GDRIVE_SA_KEY (path to service account JSON) is required");
            let base = env("GDRIVE_API_BASE").unwrap_or_else(|| "https://www.googleapis.com".into());
            let secs: u64 = env("REFRESH_SECS").and_then(|s| s.parse().ok()).unwrap_or(60);
            let drive = Drive::new(&key, &folder, &base).unwrap_or_else(|e| panic!("{e}"));
            drive.start(Duration::from_secs(secs));
            println!("source: google drive folder {folder}");
            Source::Drive(drive)
        }
        None => {
            let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "apps".into()));
            println!("source: local dir {}", root.display());
            Source::Local(root)
        }
    };
    let source = Arc::new(source);

    let addr = env("ADDR").unwrap_or_else(|| "127.0.0.1:8000".into());
    let server = Server::http(&addr).expect("failed to bind");
    println!("listening on http://{addr}");

    for req in server.incoming_requests() {
        let source = Arc::clone(&source);
        thread::spawn(move || handle(req, &source));
    }
}
