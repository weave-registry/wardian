//! The Splunk port over Splunk's REST API, with the account's certificate settings.

use crate::ports::splunk::{SplunkApi, SplunkConfig, SplunkError, SplunkSession};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{pem::PemObject, CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

pub struct SplunkRest;

impl SplunkApi for SplunkRest {
    fn connect(&self, cfg: &SplunkConfig) -> Result<Box<dyn SplunkSession>, String> {
        Ok(Box::new(Session { agent: agent(cfg)?, base: cfg.url.clone(), auth: cfg.auth() }))
    }
}

struct Session {
    agent: ureq::Agent,
    base: String,
    auth: String,
}

impl Session {
    fn reply(r: ureq::Response, url: &str) -> Result<Value, SplunkError> {
        r.into_json().map_err(|e| SplunkError::Unreadable { url: url.to_string(), reason: e.to_string() })
    }
}

impl SplunkSession for Session {
    fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, SplunkError> {
        let url = self.url(path);
        let mut req = self.agent.get(&url);
        for (k, v) in query {
            req = req.query(k, v);
        }
        let r = req.set("Authorization", &self.auth).call().map_err(|e| SplunkError::Call(splunk_error(e, "GET", &url)))?;
        Session::reply(r, &url)
    }

    fn post(&self, path: &str, form: &[(&str, &str)]) -> Result<Value, SplunkError> {
        let url = self.url(path);
        let r = self.agent.post(&url).set("Authorization", &self.auth).send_form(form).map_err(|e| SplunkError::Call(splunk_error(e, "POST", &url)))?;
        Session::reply(r, &url)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }
}

/// A full reason for a failed Splunk call: what Wardian asked for, what came
/// back, Splunk's own words, and a hint for the usual causes. Also printed in
/// the server log, which has room for the whole reply.
fn splunk_error(e: ureq::Error, method: &str, url: &str) -> String {
    let msg = match e {
        ureq::Error::Status(code, resp) => {
            let server = resp.header("Server").unwrap_or("").to_string();
            let body = resp.into_string().unwrap_or_default();
            eprintln!("splunk: {method} {url} -> {code} (server: {server})\n{}", clip(&body, 4000));
            let said = reply_text(&body);
            let hint = match code {
                401 => " The token or password is wrong or expired. A token also needs token authentication turned on in Splunk (Settings → Tokens).".to_string(),
                403 => " The account has no right to do this. Its role needs the search capability.".to_string(),
                404 => hint_404(url, &server, &body),
                _ => String::new(),
            };
            format!(
                "{method} {url} returned {code}{}{}.{hint}",
                if server.is_empty() { String::new() } else { format!(" from \"{server}\"") },
                if said.is_empty() { String::new() } else { format!(": {said}") },
            )
        }
        ureq::Error::Transport(t) => {
            let text = t.to_string();
            eprintln!("splunk: {method} {url} -> {text}");
            if text.contains("UnsupportedCertVersion") {
                format!("{url} uses an old-format (X.509 version 1) certificate, such as Splunk's default SplunkServerDefaultCert. Tick \"Allow a self-signed certificate\", or ask the Splunk admin for a proper certificate.")
            } else if text.contains("UnknownIssuer") || text.contains("NotValidForName") || text.contains("certificate") {
                format!("cannot trust the certificate of {url} ({text}). Give the CA file, or tick \"Allow a self-signed certificate\".")
            } else if text.contains("InvalidContentType") || text.contains("CorruptMessage") || text.contains("record") {
                format!("{url}: the TLS handshake failed ({text}). If this port speaks plain HTTP, use http:// instead of https://.")
            } else {
                format!("cannot reach {url}: {text}. Check the host name, the port (8089), and that a firewall lets this machine in.")
            }
        }
    };
    msg
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

fn agent(cfg: &SplunkConfig) -> Result<ureq::Agent, String> {
    // No limit on a whole request: a slow reply is fine as long as data keeps coming.
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(120))
        .timeout_write(Duration::from_secs(60));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    if cfg.insecure_tls {
        let tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCert(provider)))
            .with_no_client_auth();
        b = b.tls_config(Arc::new(tls));
    } else if !cfg.ca_file.is_empty() {
        let mut roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let certs = CertificateDer::pem_file_iter(&cfg.ca_file).map_err(|e| format!("CA file {}: {e}", cfg.ca_file))?;
        for cert in certs {
            roots.add(cert.map_err(|e| format!("CA file {}: {e}", cfg.ca_file))?).map_err(|e| e.to_string())?;
        }
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        b = b.tls_config(Arc::new(tls));
    }
    Ok(b.build())
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
