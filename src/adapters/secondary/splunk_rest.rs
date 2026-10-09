//! The Splunk port over Splunk's REST API, with the account's certificate settings.
//!
//! A failed call says what kind of failure it is (ADR-2610091500): not Splunk's API (a web page, a
//! redirect, a 404), nothing usable answering, Splunk refusing the account, or a certificate
//! Wardian cannot trust, with the certificate's subject and issuer read during the handshake.

use crate::ports::splunk::{cert_names, CertNames, SplunkApi, SplunkConfig, SplunkError, SplunkSession};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct SplunkRest;

impl SplunkApi for SplunkRest {
    fn connect(&self, cfg: &SplunkConfig) -> Result<Box<dyn SplunkSession>, String> {
        let (agent, refused) = agent(cfg)?;
        Ok(Box::new(Session { agent, base: cfg.url.clone(), auth: cfg.auth(), refused }))
    }
}

/// The certificate the verifier last refused, and why: (the end-entity certificate, the reason).
type Refused = Arc<Mutex<Option<(Vec<u8>, String)>>>;

struct Session {
    agent: ureq::Agent,
    base: String,
    auth: String,
    refused: Refused,
}

impl Session {
    /// A reply that is Splunk's API answers JSON. A redirect or a web page is something else on
    /// the same host, usually Splunk Web.
    fn reply(&self, r: ureq::Response, url: &str) -> Result<Value, SplunkError> {
        if (300..400).contains(&r.status()) {
            let to = r.header("Location").unwrap_or("another page").to_string();
            return Err(SplunkError::NotTheApi(format!("{} sends the browser on to {}, a web page, not Splunk's API", self.base, clip(&to, 120))));
        }
        let html = r.content_type() == "text/html";
        r.into_json().map_err(|e| {
            if html {
                SplunkError::NotTheApi(format!("{} answers with a web page, not Splunk's API", self.base))
            } else {
                SplunkError::Unreadable { url: url.to_string(), reason: e.to_string() }
            }
        })
    }

    fn failed(&self, e: ureq::Error, method: &str, url: &str) -> SplunkError {
        let refused = self.refused.lock().unwrap().take();
        splunk_error(e, method, url, &self.base, refused)
    }
}

impl SplunkSession for Session {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, SplunkError> {
        let url = self.url(path);
        let mut req = self.agent.get(&url);
        for (k, v) in query {
            req = req.query(k, v);
        }
        let r = req.set("Authorization", &self.auth).call().map_err(|e| self.failed(e, "GET", &url))?;
        self.reply(r, &url)
    }

    fn post(&self, path: &str, form: &[(&str, &str)]) -> Result<Value, SplunkError> {
        let url = self.url(path);
        let r = self.agent.post(&url).set("Authorization", &self.auth).send_form(form).map_err(|e| self.failed(e, "POST", &url))?;
        self.reply(r, &url)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

/// A full reason for a failed Splunk call, and its kind: what Wardian asked for, what came back,
/// Splunk's own words, and a hint for the usual causes. Also printed in the server log, which has
/// room for the whole reply. `refused` is the certificate the verifier refused, if it did.
fn splunk_error(e: ureq::Error, method: &str, url: &str, base: &str, refused: Option<(Vec<u8>, String)>) -> SplunkError {
    match e {
        ureq::Error::Status(code, resp) => {
            let server = resp.header("Server").unwrap_or("").to_string();
            let html = resp.content_type() == "text/html";
            let body = resp.into_string().unwrap_or_default();
            eprintln!("splunk: {method} {url} -> {code} (server: {server})\n{}", clip(&body, 4000));
            let lower = body.trim_start().to_ascii_lowercase();
            let page = html || lower.starts_with("<!doctype html") || lower.starts_with("<html") || lower.contains("<body");
            let said = reply_text(&body);
            if page {
                // Splunk Web answers /services/... with its own "Page not found!" page.
                return SplunkError::NotTheApi(format!("{base} answers with a web page, not Splunk's API ({method} {url} returned {code}{})", if said.is_empty() { String::new() } else { format!(": {}", clip(&said, 80)) }));
            }
            let hint = match code {
                401 => " The token or password is wrong or expired. A token also needs token authentication turned on in Splunk (Settings → Tokens).".to_string(),
                403 => " The account has no right to do this. Its role needs the search capability.".to_string(),
                404 => hint_404(url, &server, &body),
                _ => String::new(),
            };
            let text = format!(
                "{method} {url} returned {code}{}{}.{hint}",
                if server.is_empty() { String::new() } else { format!(" from \"{server}\"") },
                if said.is_empty() { String::new() } else { format!(": {said}") },
            );
            match code {
                401 | 403 => SplunkError::Refused(text),
                404 => SplunkError::NotTheApi(text),
                _ => SplunkError::Call(text),
            }
        }
        ureq::Error::Transport(t) => {
            let text = t.to_string();
            eprintln!("splunk: {method} {url} -> {text}");
            if let Some((der, why)) = refused {
                return certificate_error(base, cert_names(&der), &why);
            }
            if text.contains("UnsupportedCertVersion") || text.contains("UnknownIssuer") || text.contains("NotValidForName") || text.contains("certificate") {
                certificate_error(base, None, &text)
            } else if text.contains("InvalidContentType") || text.contains("CorruptMessage") || text.contains("record") {
                SplunkError::Unreachable(format!("{url}: the TLS handshake failed ({text}). If this port speaks plain HTTP, use http:// instead of https://."))
            } else {
                SplunkError::Unreachable(format!("cannot reach {url}: {text}. Check the host name, the port (usually 8089), and that a firewall lets this machine in."))
            }
        }
    }
}

/// A certificate Wardian cannot trust: whose it is, and what to do about it. Splunk's own
/// built-in certificate is named as such, since every Splunk ships with it.
fn certificate_error(base: &str, names: Option<CertNames>, why: &str) -> SplunkError {
    let text = match &names {
        Some(n) if n.is_splunk_default() => format!(
            "{base} has Splunk's own built-in certificate ({}, issued by {}), which no computer trusts by itself. If this is your Splunk, trust Splunk's own certificate (\"Allow a self-signed certificate\"), or give the CA file of your Splunk.",
            n.subject, n.issuer
        ),
        Some(n) => format!(
            "Wardian cannot trust the certificate of {base} ({}, issued by {}): {why}. Give the certificate of the authority that issued it, as a PEM file, in \"CA file\".",
            if n.subject.is_empty() { "no name" } else { &n.subject },
            if n.issuer.is_empty() { "an unnamed authority" } else { &n.issuer },
        ),
        None => format!("Wardian cannot trust the certificate of {base}: {why}. Give the certificate of the authority that issued it, as a PEM file, in \"CA file\", or, for Splunk's own certificate, tick \"Allow a self-signed certificate\"."),
    };
    SplunkError::Certificate { text, names }
}

fn hint_404(url: &str, server: &str, body: &str) -> String {
    let port = url.split("://").nth(1).and_then(|r| r.split('/').next()).and_then(|h| h.rsplit_once(':')).map(|(_, p)| p);
    let lower = body.to_ascii_lowercase();
    if port == Some("8000") || lower.contains("<html") && !server.to_ascii_lowercase().contains("splunkd") {
        " This looks like a web page, not Splunk's REST API. Use the management port, usually https://<host>:8089, not the web port 8000.".into()
    } else if port == Some("8088") || lower.contains("hec") {
        " Port 8088 is the HTTP Event Collector, which only takes data in. Searches need the management port, usually 8089.".into()
    } else if url.contains("splunkcloud.com") {
        " On Splunk Cloud, REST API access must be turned on for your stack, and the address is https://<stack>.splunkcloud.com:8089.".into()
    } else {
        " Check that the address is Splunk's management port (usually 8089) and that no proxy in between rewrites the path.".into()
    }
}

/// Splunk's own words from a reply: messages[].text in JSON or <msg> in XML,
/// or else the reply's text without markup.
fn reply_text(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        let texts: Vec<&str> = v["messages"].as_array().map(|a| a.iter().filter_map(|m| m["text"].as_str()).collect()).unwrap_or_default();
        if !texts.is_empty() {
            return texts.join("; ");
        }
    }
    if let Some(start) = body.find("<msg") {
        if let Some(open_end) = body[start..].find('>') {
            let rest = &body[start + open_end + 1..];
            if let Some(close) = rest.find("</msg>") {
                return rest[..close].trim().to_string();
            }
        }
    }
    let mut out = String::new();
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => in_tag = true,
            '>' => { in_tag = false; out.push(' '); }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    clip(&out.split_whitespace().collect::<Vec<_>>().join(" "), 300)
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

/// An agent for this account's certificate settings, and where its verifier puts a certificate it
/// refuses. Redirects are not followed: Splunk's API never redirects, and Splunk Web sends a
/// browser on to its login page, which is not the API.
fn agent(cfg: &SplunkConfig) -> Result<(ureq::Agent, Refused), String> {
    // No limit on a whole request: a slow reply is fine as long as data keeps coming.
    let b = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(120))
        .timeout_write(Duration::from_secs(60))
        .redirects(0);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let refused = Refused::default();
    let verifier: Arc<dyn ServerCertVerifier> = if cfg.insecure_tls {
        Arc::new(AnyCert(provider.clone()))
    } else {
        let mut roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        if !cfg.ca_file.is_empty() {
            let certs = CertificateDer::pem_file_iter(&cfg.ca_file).map_err(|e| format!("CA file {}: {e}", cfg.ca_file))?;
            for cert in certs {
                roots.add(cert.map_err(|e| format!("CA file {}: {e}", cfg.ca_file))?).map_err(|e| e.to_string())?;
            }
        }
        let inner = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone()).build().map_err(|e| e.to_string())?;
        Arc::new(Recording { inner, refused: Arc::clone(&refused) })
    };
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok((b.tls_config(Arc::new(tls)).build(), refused))
}

/// The usual certificate check, which also keeps the certificate it refuses, so that the message
/// can say whose it is: Splunk's own, or another authority's (ADR-2610091500).
#[derive(Debug)]
struct Recording {
    inner: Arc<WebPkiServerVerifier>,
    refused: Refused,
}

impl ServerCertVerifier for Recording {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, intermediates: &[CertificateDer<'_>], name: &ServerName<'_>, ocsp: &[u8], now: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        self.inner.verify_server_cert(end_entity, intermediates, name, ocsp, now).inspect_err(|e| {
            *self.refused.lock().unwrap() = Some((end_entity.to_vec(), e.to_string()));
        })
    }
    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Trusts any certificate. Only used when the admin ticks "allow a
/// self-signed certificate".
///
/// It also skips the handshake signature check. That check reads the
/// certificate with webpki, which refuses X.509 version 1, and Splunk's own
/// default certificate (SplunkServerDefaultCert) is version 1. Skipping it
/// loses nothing here: a verifier that accepts any certificate already
/// accepts an attacker's, and the attacker can sign with that one.
#[derive(Debug)]
struct AnyCert(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCert {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
