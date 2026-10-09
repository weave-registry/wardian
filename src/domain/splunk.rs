//! Splunk, as Wardian sees it: the account settings, the search text, and the table a search
//! returns to an app. The calls themselves go through the Splunk port.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

/// The most rows one search returns to an app.
const MAX_ROWS: usize = 10_000;
/// The longest Wardian waits for one search before it cancels the job.
pub const MAX_SEARCH_TIME: Duration = Duration::from_secs(15 * 60);
const MAX_SEARCH_CHARS: usize = 10_000;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct SplunkConfig {
    /// The management address, e.g. https://splunk.example.com:8089
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    /// Accept any certificate. For Splunk's own self-signed default certificate.
    #[serde(default)]
    pub insecure_tls: bool,
    /// A PEM file of extra certificate authorities to trust.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ca_file: String,
}

impl SplunkConfig {
    /// Tidies the address and says what is missing.
    pub fn check(&mut self) -> Result<(), String> {
        self.url = SplunkAddress::read(&self.url)?.url();
        self.token = self.token.trim().to_string();
        self.username = self.username.trim().to_string();
        self.ca_file = self.ca_file.trim().to_string();
        if self.token.is_empty() && (self.username.is_empty() || self.password.is_empty()) {
            return Err("give a Splunk token, or a username and password".into());
        }
        Ok(())
    }

    /// The Authorization header value.
    pub fn auth(&self) -> String {
        if !self.token.is_empty() {
            format!("Bearer {}", self.token)
        } else {
            format!("Basic {}", base64(format!("{}:{}", self.username, self.password).as_bytes()))
        }
    }
}

/// Splunk needs a search to start with a command. Most people type the
/// filter only ("index=x ..."), which means "search index=x ...".
pub fn normalize_search(spl: &str) -> Result<String, String> {
    let spl = spl.trim();
    if spl.is_empty() {
        return Err("the search is empty".into());
    }
    if spl.chars().count() > MAX_SEARCH_CHARS {
        return Err(format!("a search may be at most {MAX_SEARCH_CHARS} characters"));
    }
    let lower = spl.to_ascii_lowercase();
    Ok(if spl.starts_with('|') || lower.starts_with("search ") { spl.to_string() } else { format!("search {spl}") })
}

/// Turns a oneshot reply into columns and rows, in the order the search
/// named its fields. Internal fields (_raw, _time …) are left out unless the
/// search keeps nothing else. A multivalue cell keeps its first value.
pub fn table(body: &Value) -> Value {
    let fields = fields_of(body);
    let results = body["results"].as_array().cloned().unwrap_or_default();
    let truncated = results.len() > MAX_ROWS;
    let rows: Vec<Value> = rows_of(body, &fields).into_iter().take(MAX_ROWS).map(Value::Array).collect();
    let messages: Vec<Value> = body["messages"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["text"].as_str().map(|t| json!(t))).collect())
        .unwrap_or_default();
    json!({ "fields": fields, "rows": rows, "truncated": truncated, "messages": messages })
}

/// The columns of a results reply, in the order the search named them. Internal fields (_raw,
/// _time …) are left out unless the search keeps nothing else.
pub fn fields_of(body: &Value) -> Vec<String> {
    let mut fields: Vec<String> = body["fields"]
        .as_array()
        .map(|a| a.iter().filter_map(|f| f["name"].as_str().or_else(|| f.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    if fields.is_empty() {
        if let Some(first) = body["results"].as_array().and_then(|r| r.first()).and_then(Value::as_object) {
            fields = first.keys().cloned().collect();
        }
    }
    let visible: Vec<String> = fields.iter().filter(|f| !f.starts_with('_')).cloned().collect();
    if visible.is_empty() {
        fields
    } else {
        visible
    }
}

/// The rows of a results reply, one cell per field; a multivalue cell keeps its first value.
pub fn rows_of(body: &Value, fields: &[String]) -> Vec<Vec<Value>> {
    body["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .map(|r| {
                    fields
                        .iter()
                        .map(|f| match &r[f] {
                            Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
                            v => v.clone(),
                        })
                        .collect()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The port Splunk's management API listens on unless its admin changed it.
pub const API_PORT: u16 = 8089;

/// A Splunk address read generously (ADR-2610091500). People paste what their browser shows,
/// such as `https://host/en-US/app/search/search?q=...`, which is Splunk Web. Only the scheme,
/// the host and the port are kept; every call adds its own path.
#[derive(Clone, PartialEq, Debug)]
pub struct SplunkAddress {
    /// "https" or "http"; https when none is given.
    pub scheme: String,
    /// A host name, an IPv4 address, or an IPv6 address in brackets.
    pub host: String,
    /// The port, when one is given.
    pub port: Option<u16>,
}

impl SplunkAddress {
    pub fn read(input: &str) -> Result<SplunkAddress, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("type your Splunk's address, as in your browser".into());
        }
        let (scheme, rest) = match input.split_once("://") {
            Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
            None => ("https".to_string(), input),
        };
        if scheme != "https" && scheme != "http" {
            return Err(format!("\"{input}\" is not a Splunk address: Wardian talks to Splunk over https:// (or http://), not {scheme}://"));
        }
        // The part before the path. A "user@" in front is dropped, and never sent.
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
        let (host, port) = split_host_port(authority).ok_or_else(|| not_an_address(input))?;
        let port = match port {
            None => None,
            Some(p) => match p.parse::<u16>() {
                Ok(n) if n > 0 && p.bytes().all(|c| c.is_ascii_digit()) => Some(n),
                _ => return Err(format!("\"{p}\" in \"{input}\" is not a port: a port is a number from 1 to 65535, such as {API_PORT}")),
            },
        };
        Ok(SplunkAddress { scheme, host, port })
    }

    /// The address as Wardian saves it: scheme, host and port, no path.
    pub fn url(&self) -> String {
        match self.port {
            Some(p) => format!("{}://{}:{p}", self.scheme, self.host),
            None => format!("{}://{}", self.scheme, self.host),
        }
    }

    /// The addresses setup tries, in order (ADR-2610091500): the address as given (with the port
    /// typed, if any), then the same host on Splunk's API port over https, then, when the address
    /// was http://, over http as well. No other host or port, and never one address twice.
    pub fn candidates(&self, api_port: u16) -> Vec<String> {
        let api = |scheme: &str| SplunkAddress { scheme: scheme.into(), host: self.host.clone(), port: Some(api_port) }.url();
        let mut out = vec![self.url()];
        for url in [api("https"), api(&self.scheme)] {
            if !out.contains(&url) {
                out.push(url);
            }
        }
        out
    }
}

fn not_an_address(input: &str) -> String {
    format!("\"{input}\" is not a Splunk address. Type its host name, as in https://splunk.example.com, or paste the address from your browser")
}

/// The host (lower case; an IPv6 address in brackets) and the port text of `authority`.
fn split_host_port(authority: &str) -> Option<(String, Option<&str>)> {
    let (host, port) = if let Some(after) = authority.strip_prefix('[') {
        let (ip, tail) = after.split_once(']')?;
        if !is_ipv6(ip) {
            return None;
        }
        let port = if tail.is_empty() { None } else { Some(tail.strip_prefix(':')?) };
        (format!("[{}]", ip.to_ascii_lowercase()), port)
    } else if is_ipv6(authority) {
        (format!("[{}]", authority.to_ascii_lowercase()), None)
    } else {
        match authority.split_once(':') {
            Some((h, p)) => (h.to_ascii_lowercase(), Some(p)),
            None => (authority.to_ascii_lowercase(), None),
        }
    };
    let plain = |h: &str| {
        !h.is_empty() && h.len() <= 253 && !h.starts_with(['-', '.']) && h.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_'))
    };
    (host.starts_with('[') || plain(&host)).then_some((host, port))
}

/// An IPv6 address in text: eight groups of up to four hex digits, or fewer around one "::", the
/// last two of which may be an IPv4 address.
fn is_ipv6(s: &str) -> bool {
    // How many 16-bit groups a run of "a:b:c" holds, or None when it is not one.
    let groups = |run: &str| -> Option<usize> {
        if run.is_empty() {
            return Some(0);
        }
        let parts: Vec<&str> = run.split(':').collect();
        let mut n = 0;
        for (i, g) in parts.iter().enumerate() {
            if i + 1 == parts.len() && g.contains('.') {
                let octets: Vec<&str> = g.split('.').collect();
                let ok = octets.len() == 4 && octets.iter().all(|o| !o.is_empty() && o.len() <= 3 && o.bytes().all(|c| c.is_ascii_digit()) && o.parse::<u16>().is_ok_and(|v| v <= 255));
                n += if ok { 2 } else { return None };
            } else if (1..=4).contains(&g.len()) && g.bytes().all(|c| c.is_ascii_hexdigit()) {
                n += 1;
            } else {
                return None;
            }
        }
        Some(n)
    };
    if !s.contains(':') {
        return false;
    }
    match s.split_once("::") {
        Some((head, tail)) => !tail.contains("::") && matches!((groups(head), groups(tail)), (Some(h), Some(t)) if h + t <= 7),
        None => groups(s) == Some(8),
    }
}

/// The port of a saved address ("https://host:8089" → 8089), for messages.
pub fn port_of(url: &str) -> Option<u16> {
    SplunkAddress::read(url).ok().and_then(|a| a.port)
}

/// Who a server certificate names: its subject and its issuer, each as "CN=…, O=…".
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CertNames {
    pub subject: String,
    pub issuer: String,
}

impl CertNames {
    /// Splunk's built-in certificate: SplunkServerDefaultCert, issued by SplunkCommonCA. Every
    /// Splunk ships with it, and no computer trusts it by itself.
    pub fn is_splunk_default(&self) -> bool {
        let cn = |dn: &str| dn.split(", ").find_map(|p| p.strip_prefix("CN=")).unwrap_or("").to_string();
        cn(&self.issuer) == "SplunkCommonCA" || cn(&self.subject) == "SplunkServerDefaultCert"
    }
}

/// Reads the subject and the issuer of a DER certificate (X.509 version 1 or 3), keeping the
/// common name and the organization. None when the bytes are not a certificate.
pub fn cert_names(der: &[u8]) -> Option<CertNames> {
    let (cert, _) = der_item(der, 0x30)?;
    let (mut tbs, _) = der_item(cert, 0x30)?;
    if tbs.first() == Some(&0xa0) {
        tbs = der_item(tbs, 0xa0)?.1; // the version, absent in version 1
    }
    let rest = der_item(tbs, 0x02)?.1; // the serial number
    let rest = der_item(rest, 0x30)?.1; // the signature algorithm
    let (issuer, rest) = der_item(rest, 0x30)?;
    let rest = der_item(rest, 0x30)?.1; // the validity
    let (subject, _) = der_item(rest, 0x30)?;
    Some(CertNames { subject: dn_text(subject), issuer: dn_text(issuer) })
}

/// The common names and organizations of a distinguished name, as "CN=…, O=…".
fn dn_text(mut name: &[u8]) -> String {
    const CN: &[u8] = &[0x55, 0x04, 0x03];
    const O: &[u8] = &[0x55, 0x04, 0x0a];
    let (mut cn, mut o) = (Vec::new(), Vec::new());
    while let Some((mut set, rest)) = der_item(name, 0x31) {
        name = rest;
        while let Some((pair, more)) = der_item(set, 0x30) {
            set = more;
            let Some((oid, value)) = der_item(pair, 0x06) else { continue };
            let Some(text) = value.first().and_then(|&tag| der_item(value, tag)).map(|(t, _)| String::from_utf8_lossy(t).into_owned()) else { continue };
            if oid == CN {
                cn.push(format!("CN={text}"));
            } else if oid == O {
                o.push(format!("O={text}"));
            }
        }
    }
    cn.into_iter().chain(o).collect::<Vec<_>>().join(", ")
}

/// The DER item tagged `tag` at the start of `input`: its contents, and what follows it.
fn der_item(input: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    let (&t, rest) = input.split_first()?;
    let (&first, rest) = rest.split_first()?;
    if t != tag {
        return None;
    }
    let (len, rest) = if first < 0x80 {
        (usize::from(first), rest)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 || rest.len() < n {
            return None;
        }
        (rest[..n].iter().fold(0usize, |acc, &b| acc << 8 | usize::from(b)), &rest[n..])
    };
    (rest.len() >= len).then(|| rest.split_at(len))
}


fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(input: &str) -> Result<String, String> {
        SplunkAddress::read(input).map(|a| a.url())
    }

    #[test]
    fn splunk_setup_reads_a_host_name_alone_as_https() {
        assert_eq!(read("splunk.example.com").unwrap(), "https://splunk.example.com");
        assert_eq!(read("  Splunk.Example.COM  ").unwrap(), "https://splunk.example.com");
        assert_eq!(read("10.1.2.3:8089").unwrap(), "https://10.1.2.3:8089");
    }

    #[test]
    fn splunk_setup_keeps_http_and_https() {
        assert_eq!(read("http://splunk.lab:8000").unwrap(), "http://splunk.lab:8000");
        assert_eq!(read("HTTPS://splunk.lab").unwrap(), "https://splunk.lab");
    }

    #[test]
    fn splunk_setup_drops_the_path_and_query_of_a_browser_address() {
        assert_eq!(read("https://splunk.ext.example.com/en-US/app/search/search?q=index%3Dmain#x").unwrap(), "https://splunk.ext.example.com");
        assert_eq!(read("https://splunk.example.com:8089/services/").unwrap(), "https://splunk.example.com:8089");
        assert_eq!(read("https://splunk.example.com?x=1").unwrap(), "https://splunk.example.com");
        assert_eq!(read("https://admin@splunk.example.com/").unwrap(), "https://splunk.example.com", "a user name is never kept");
    }

    #[test]
    fn splunk_setup_reads_an_ipv6_literal() {
        assert_eq!(read("https://[2001:DB8::1]:8090/en-US/").unwrap(), "https://[2001:db8::1]:8090");
        assert_eq!(read("[::1]").unwrap(), "https://[::1]");
        assert_eq!(read("2001:db8::7").unwrap(), "https://[2001:db8::7]");
        assert_eq!(read("[1:2:3:4:5:6:7:8]:8089").unwrap(), "https://[1:2:3:4:5:6:7:8]:8089");
        assert_eq!(read("http://[::ffff:10.0.0.1]/x").unwrap(), "http://[::ffff:10.0.0.1]");
        for bad in ["[1:2:3:4:5:6:7:8:9]", "[1::2::3]", "[12345::1]", "[fe80::1%en0]", "[::ffff:10.0.0.256]", "[]"] {
            assert!(read(bad).is_err(), "{bad} is not an IPv6 address");
        }
        assert_eq!(SplunkAddress::read("https://[::1]:8000").unwrap().candidates(API_PORT), ["https://[::1]:8000", "https://[::1]:8089"]);
    }

    #[test]
    fn splunk_setup_tries_a_typed_port_first_then_8089() {
        let a = SplunkAddress::read("https://splunk.example.com:8090/x").unwrap();
        assert_eq!(a.port, Some(8090));
        assert_eq!(a.candidates(API_PORT), ["https://splunk.example.com:8090", "https://splunk.example.com:8089"]);
        let a = SplunkAddress::read("https://splunk.example.com/en-US/app/search").unwrap();
        assert_eq!(a.candidates(API_PORT), ["https://splunk.example.com", "https://splunk.example.com:8089"]);
    }

    #[test]
    fn splunk_setup_never_tries_an_address_twice() {
        assert_eq!(SplunkAddress::read("splunk.example.com:8089").unwrap().candidates(API_PORT), ["https://splunk.example.com:8089"]);
        // http:// tries the API port over https first, which is how Splunk ships, then over http.
        assert_eq!(SplunkAddress::read("http://s:8000").unwrap().candidates(API_PORT), ["http://s:8000", "https://s:8089", "http://s:8089"]);
        assert_eq!(SplunkAddress::read("http://s:8089").unwrap().candidates(API_PORT), ["http://s:8089", "https://s:8089"]);
        assert_eq!(SplunkAddress::read("https://s:9089").unwrap().candidates(9089), ["https://s:9089"]);
    }

    #[test]
    fn splunk_setup_refuses_junk_with_a_clear_message() {
        for (input, says) in [
            ("", "type your Splunk's address"),
            ("ftp://splunk.example.com", "not ftp://"),
            ("https://", "is not a Splunk address"),
            ("splunk example com", "is not a Splunk address"),
            ("https://splunk.example.com:80x9", "is not a port"),
            ("https://splunk.example.com:0", "is not a port"),
            ("https://splunk.example.com:70000", "is not a port"),
            ("https://[not-ipv6]:8089", "is not a Splunk address"),
            ("https://-bad.example.com", "is not a Splunk address"),
        ] {
            let e = read(input).unwrap_err();
            assert!(e.contains(says), "{input:?}: {e}");
        }
    }

    #[test]
    fn splunk_setup_check_tidies_the_address() {
        let mut c = SplunkConfig { url: "https://splunk.example.com/en-US/app/search".into(), token: " t ".into(), ..Default::default() };
        c.check().unwrap();
        assert_eq!((c.url.as_str(), c.token.as_str()), ("https://splunk.example.com", "t"));
        assert_eq!(port_of("https://s:8089"), Some(8089));
        assert_eq!(port_of("https://s"), None);
    }

    /// A DER item: tag, length, contents.
    fn der(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        if body.len() < 0x80 {
            out.push(body.len() as u8);
        } else {
            out.extend([0x82, (body.len() >> 8) as u8, body.len() as u8]);
        }
        out.extend_from_slice(body);
        out
    }

    /// A name of a country, an O and a CN, as a certificate carries it.
    fn name(cn: &str, o: &str) -> Vec<u8> {
        let rdn = |oid: u8, tag: u8, v: &str| der(0x31, &der(0x30, &[der(0x06, &[0x55, 0x04, oid]), der(tag, v.as_bytes())].concat()));
        der(0x30, &[rdn(0x06, 0x13, "US"), rdn(0x0a, 0x0c, o), rdn(0x03, 0x13, cn)].concat())
    }

    /// The front of a certificate, enough for `cert_names`, with or without a version.
    fn cert(v3: bool, subject: &[u8], issuer: &[u8]) -> Vec<u8> {
        let version = if v3 { der(0xa0, &der(0x02, &[2])) } else { Vec::new() };
        let tbs = [version, der(0x02, &[1, 2, 3]), der(0x30, &der(0x06, &[0x2a, 0x86, 0x48])), issuer.to_vec(), der(0x30, &[0u8; 30]), subject.to_vec(), der(0x30, &[0u8; 200])].concat();
        der(0x30, &[der(0x30, &tbs), der(0x30, &[]), der(0x03, &[0, 1])].concat())
    }

    #[test]
    fn splunk_setup_names_splunks_own_certificate() {
        for v3 in [false, true] {
            let names = cert_names(&cert(v3, &name("SplunkServerDefaultCert", "SplunkUser"), &name("SplunkCommonCA", "Splunk"))).unwrap();
            assert_eq!(names.subject, "CN=SplunkServerDefaultCert, O=SplunkUser");
            assert_eq!(names.issuer, "CN=SplunkCommonCA, O=Splunk");
            assert!(names.is_splunk_default());
        }
        let other = cert_names(&cert(true, &name("splunk.corp.example", "Example"), &name("Example Issuing CA 2", "Example"))).unwrap();
        assert_eq!(other.issuer, "CN=Example Issuing CA 2, O=Example");
        assert!(!other.is_splunk_default());
        assert_eq!(cert_names(b"not a certificate"), None);
        assert_eq!(cert_names(&[0x30, 0x84, 0xff, 0xff, 0xff, 0xff]), None, "a length past the end is refused");
    }
}
