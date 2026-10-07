//! HTTP/1.1 over TCP, small enough to read in one sitting: one thread per connection, keep-alive
//! with an idle limit, Content-Length and chunked bodies, `Expect: 100-continue`.
//!
//! It replaces tiny_http (ADR-2610072033). tiny_http hands each new connection to a pool whose
//! count of idle threads is only lowered once a woken thread runs, so when several connections
//! arrive together the pool can count a thread twice and leave a connection queued with no thread
//! to read it. A browser keeps its connections open, so the queued one waits until another closes:
//! the page waits forever for one or two files. Here every connection gets its own thread at once.

use crate::ports::calendar::Utc;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long an open connection may sit between requests before the server closes it.
const IDLE: Duration = Duration::from_secs(120);
/// How long one read or write may wait while a request or response is under way.
const STALL: Duration = Duration::from_secs(60);
const MAX_LINE: usize = 16 * 1024;
const MAX_HEADERS: usize = 100;
/// An unread body up to this size is skipped so the connection can be reused; a larger one
/// closes the connection instead.
const MAX_SKIP: u64 = 1024 * 1024;
/// More open connections than this are answered 503 and closed.
const MAX_CONNECTIONS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Other(String),
}

/// A header, as the handler adds it to a response.
pub struct Header(String, String);

impl Header {
    pub fn new(name: &str, value: &str) -> Header {
        // Header text cannot hold a line break; drop it rather than split the response.
        let clean = |s: &str| s.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        Header(clean(name), clean(value))
    }
}

pub struct Response {
    status: u16,
    headers: Vec<Header>,
    body: Vec<u8>,
}

impl Response {
    pub fn from_data(body: Vec<u8>) -> Response {
        Response { status: 200, headers: Vec::new(), body }
    }
    pub fn from_string(body: impl Into<String>) -> Response {
        Response::from_data(body.into().into_bytes())
    }
    pub fn with_status_code(mut self, status: u16) -> Response {
        self.status = status;
        self
    }
    pub fn with_header(mut self, h: Header) -> Response {
        self.headers.push(h);
        self
    }
}

/// How the body after the headers is framed.
enum Framing {
    Length(u64),
    /// Bytes left in the current chunk; `None` before the first size line, and after the last
    /// chunk once `done`.
    Chunked { left: u64, started: bool, done: bool },
}

/// The request body, read straight from the connection.
pub struct Body<'a> {
    src: &'a mut BufReader<TcpStream>,
    framing: Framing,
}

impl Body<'_> {
    fn next_chunk(&mut self) -> io::Result<()> {
        let Framing::Chunked { left, started, done } = &mut self.framing else { return Ok(()) };
        if *started {
            // The CRLF after the previous chunk's data.
            read_line(self.src)?;
        }
        *started = true;
        let line = read_line(self.src)?;
        let size = line.split(';').next().unwrap_or("").trim();
        *left = u64::from_str_radix(size, 16).map_err(|_| bad("bad chunk size"))?;
        if *left == 0 {
            // Trailers end with an empty line.
            while !read_line(self.src)?.is_empty() {}
            *done = true;
        }
        Ok(())
    }

    /// Reads and drops what the handler left unread, up to `max` bytes. False when the body was
    /// larger or could not be read, so the connection cannot be reused.
    fn skip_rest(&mut self, max: u64) -> bool {
        let mut sink = io::sink();
        match io::copy(&mut self.take(max + 1), &mut sink) {
            Ok(n) if n <= max => self.finished(),
            _ => false,
        }
    }

    fn finished(&self) -> bool {
        match self.framing {
            Framing::Length(n) => n == 0,
            Framing::Chunked { done, .. } => done,
        }
    }
}

impl Read for Body<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let left = match &self.framing {
                Framing::Length(n) => *n,
                Framing::Chunked { done: true, .. } => return Ok(0),
                Framing::Chunked { left: 0, .. } => {
                    self.next_chunk()?;
                    continue;
                }
                Framing::Chunked { left, .. } => *left,
            };
            if left == 0 {
                return Ok(0);
            }
            let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
            let n = self.src.read(&mut buf[..want])?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the connection closed in the middle of the body"));
            }
            match &mut self.framing {
                Framing::Length(l) | Framing::Chunked { left: l, .. } => *l -= n as u64,
            }
            return Ok(n);
        }
    }
}

pub struct Request<'a> {
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    remote: Option<SocketAddr>,
    body: Body<'a>,
}

impl<'a> Request<'a> {
    pub fn method(&self) -> &Method {
        &self.method
    }
    /// The request target as sent: path and query.
    pub fn url(&self) -> &str {
        &self.url
    }
    /// The first header with this name, in any case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn remote_addr(&self) -> Option<&SocketAddr> {
        self.remote.as_ref()
    }
    pub fn as_reader(&mut self) -> &mut Body<'a> {
        &mut self.body
    }
}

/// What the server hands each request to.
pub type Handler = dyn Fn(&mut Request<'_>) -> Response + Send + Sync;

/// A bound listening socket, not yet answering.
pub struct Server {
    listener: TcpListener,
}

impl Server {
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Server> {
        Ok(Server { listener: TcpListener::bind(addr)? })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Answers connections until accepting fails, which it does only when the system refuses.
    pub fn run(self, handler: Arc<Handler>) -> io::Error {
        let open = Arc::new(AtomicUsize::new(0));
        loop {
            let stream = match self.listener.accept() {
                Ok((s, _)) => s,
                // A connection reset before it was accepted, or a brief shortage of descriptors:
                // carry on with the next.
                Err(e) if is_transient(&e) => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => return e,
            };
            if open.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                open.fetch_sub(1, Ordering::AcqRel);
                let mut s = stream;
                let _ = s.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            let (handler, open) = (Arc::clone(&handler), Arc::clone(&open));
            let spawned = thread::Builder::new().name("http-conn".into()).spawn(move || {
                serve_connection(stream, &*handler);
                open.fetch_sub(1, Ordering::AcqRel);
            });
            if let Err(e) = spawned {
                eprintln!("http: cannot start a thread for a connection: {e}");
            }
        }
    }
}

fn is_transient(e: &io::Error) -> bool {
    use io::ErrorKind::*;
    matches!(e.kind(), ConnectionAborted | ConnectionReset | Interrupted | WouldBlock | TimedOut) || e.raw_os_error().is_some_and(|c| c == 23 || c == 24)
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

/// One line without its CRLF (a bare LF is accepted too), at most MAX_LINE bytes.
fn read_line(src: &mut BufReader<TcpStream>) -> io::Result<String> {
    let mut buf = Vec::new();
    let n = src.by_ref().take(MAX_LINE as u64 + 1).read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed"));
    }
    if buf.last() != Some(&b'\n') {
        return Err(bad(if buf.len() > MAX_LINE { "line too long" } else { "closed mid-line" }));
    }
    buf.pop();
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|_| bad("not UTF-8"))
}

struct Head {
    method: Method,
    url: String,
    http10: bool,
    headers: Vec<(String, String)>,
}

fn read_head(src: &mut BufReader<TcpStream>) -> io::Result<Head> {
    // Browsers may send an empty line between requests; skip it.
    let mut line = read_line(src)?;
    while line.is_empty() {
        line = read_line(src)?;
    }
    let mut parts = line.split(' ');
    let (Some(m), Some(url), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(bad("bad request line"));
    };
    let http10 = match version {
        "HTTP/1.1" => false,
        "HTTP/1.0" => true,
        _ => return Err(bad("unsupported HTTP version")),
    };
    let method = match m {
        "GET" => Method::Get,
        "HEAD" => Method::Head,
        "POST" => Method::Post,
        other if !other.is_empty() && other.bytes().all(|b| b.is_ascii_uppercase()) => Method::Other(other.into()),
        _ => return Err(bad("bad method")),
    };
    let mut headers = Vec::new();
    loop {
        let line = read_line(src)?;
        if line.is_empty() {
            break;
        }
        if headers.len() == MAX_HEADERS {
            return Err(bad("too many headers"));
        }
        let (k, v) = line.split_once(':').ok_or_else(|| bad("bad header"))?;
        if k.is_empty() || k.contains(char::is_whitespace) {
            return Err(bad("bad header name"));
        }
        headers.push((k.to_string(), v.trim().to_string()));
    }
    Ok(Head { method, url: url.to_string(), http10, headers })
}

fn find<'h>(headers: &'h [(String, String)], name: &str) -> Option<&'h str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn serve_connection(stream: TcpStream, handler: &Handler) {
    let remote = stream.peer_addr().ok();
    let _ = stream.set_nodelay(true);
    let _ = stream.set_write_timeout(Some(STALL));
    let Ok(read_half) = stream.try_clone() else { return };
    let mut src = BufReader::new(read_half);
    let mut out = stream;
    loop {
        // Waiting for the next request on an idle connection.
        let _ = out.set_read_timeout(Some(IDLE));
        if src.buffer().is_empty() && src.fill_buf().map_or(true, |b| b.is_empty()) {
            break;
        }
        let _ = out.set_read_timeout(Some(STALL));
        let head = match read_head(&mut src) {
            Ok(h) => h,
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                let _ = write_response(&mut out, Response::from_string(format!("{e}")).with_status_code(400), false, true);
                break;
            }
            Err(_) => break,
        };
        let conn = find(&head.headers, "Connection").unwrap_or("").to_ascii_lowercase();
        let mut keep = if head.http10 { conn.contains("keep-alive") } else { !conn.contains("close") };
        let framing = if find(&head.headers, "Transfer-Encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked")) {
            Framing::Chunked { left: 0, started: false, done: false }
        } else {
            match find(&head.headers, "Content-Length").map(|v| v.parse::<u64>()) {
                None => Framing::Length(0),
                Some(Ok(n)) => Framing::Length(n),
                Some(Err(_)) => {
                    let _ = write_response(&mut out, Response::from_string("bad Content-Length").with_status_code(400), false, true);
                    break;
                }
            }
        };
        let expects_continue = !head.http10 && find(&head.headers, "Expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue"));
        if expects_continue && out.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").is_err() {
            break;
        }
        let is_head = head.method == Method::Head;
        let mut req = Request { method: head.method, url: head.url, headers: head.headers, remote, body: Body { src: &mut src, framing } };
        let resp = handler(&mut req);
        keep &= req.body.skip_rest(MAX_SKIP);
        if write_response(&mut out, resp, is_head, !keep).is_err() || !keep {
            break;
        }
    }
    let _ = out.shutdown(Shutdown::Both);
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "",
    }
}

fn write_response(out: &mut TcpStream, resp: Response, head_only: bool, close: bool) -> io::Result<()> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", resp.status, reason(resp.status));
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    head.push_str(&format!("Date: {}\r\nServer: Wardian\r\n", Utc::from_unix(now).http()));
    for Header(k, v) in &resp.headers {
        if !k.eq_ignore_ascii_case("Content-Length") && !k.eq_ignore_ascii_case("Connection") && !k.eq_ignore_ascii_case("Transfer-Encoding") {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
    }
    let no_body = resp.status == 204 || resp.status == 304 || (100..200).contains(&resp.status);
    if !no_body {
        head.push_str(&format!("Content-Length: {}\r\n", resp.body.len()));
    }
    if close {
        head.push_str("Connection: close\r\n");
    }
    head.push_str("\r\n");
    let mut msg = head.into_bytes();
    if !no_body && !head_only {
        msg.extend_from_slice(&resp.body);
    }
    out.write_all(&msg)?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    /// A server whose handler echoes the method, URL and body it got.
    fn echo_server() -> SocketAddr {
        let server = Server::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = server.local_addr().unwrap();
        thread::spawn(move || {
            server.run(Arc::new(|req: &mut Request<'_>| {
                let mut body = String::new();
                if req.url() != "/ignore-body" {
                    req.as_reader().read_to_string(&mut body).unwrap();
                }
                Response::from_string(format!("{:?} {} [{body}]", req.method(), req.url())).with_header(Header::new("X-Test", "a\r\nb"))
            }))
        });
        addr
    }

    /// Sends `raw` and reads until the server closes or `n` complete responses have arrived.
    fn exchange(addr: SocketAddr, raw: &str, n: usize) -> String {
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        s.write_all(raw.as_bytes()).unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let text = String::from_utf8_lossy(&got);
            let done = text.matches("HTTP/1.1 ").count() >= n && (text.ends_with(']') || text.ends_with("\r\n\r\n"));
            if done {
                break;
            }
            match s.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(k) => got.extend_from_slice(&buf[..k]),
            }
        }
        String::from_utf8_lossy(&got).into_owned()
    }

    #[test]
    fn keep_alive_bodies_and_framing() {
        let addr = echo_server();
        // Two requests on one connection, the first with a body the handler never reads.
        let out = exchange(addr, "POST /ignore-body HTTP/1.1\r\nContent-Length: 5\r\n\r\nhelloGET /b?x=1 HTTP/1.1\r\n\r\n", 2);
        assert!(out.contains("Post /ignore-body []") && out.contains("Get /b?x=1 []"), "{out}");
        assert!(out.contains("X-Test: ab\r\n"), "a header cannot split the response: {out}");
        // A chunked body.
        let out = exchange(addr, "POST /c HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n", 1);
        assert!(out.contains("Post /c [abcde]"), "{out}");
        // Expect: 100-continue gets its interim answer, then the real one.
        let out = exchange(addr, "POST /e HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: 2\r\n\r\nok", 2);
        assert!(out.starts_with("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK") && out.contains("[ok]"), "{out}");
        // HEAD: the length, no body; Connection: close is honoured.
        let out = exchange(addr, "HEAD /h HTTP/1.1\r\nConnection: close\r\n\r\n", 1);
        assert!(out.contains("Content-Length: 10\r\n") && out.contains("Connection: close") && out.ends_with("\r\n\r\n"), "{out}");
        // Garbage is refused and the connection closed.
        let out = exchange(addr, "NOT HTTP\r\n\r\n", 1);
        assert!(out.starts_with("HTTP/1.1 400"), "{out}");
    }

    #[test]
    fn idle_connections_do_not_block_new_ones() {
        let addr = echo_server();
        let idle: Vec<TcpStream> = (0..200).map(|_| TcpStream::connect(addr).unwrap()).collect();
        let out = exchange(addr, "GET /x HTTP/1.1\r\nConnection: close\r\n\r\n", 1);
        assert!(out.contains("Get /x []"), "{out}");
        drop(idle);
    }
}
