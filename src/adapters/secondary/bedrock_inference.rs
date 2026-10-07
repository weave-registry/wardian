//! The Claude port over Amazon Bedrock (ADR-2610071106): InvokeModel with the Anthropic Messages
//! body. This file and the Anthropic adapter are the only places that name models.
//!
//! Bedrock takes the model in the URL, not the body, and wants `anthropic_version`. It signs in
//! with a Bedrock API key (a bearer token) or AWS access keys signed with Signature Version 4,
//! done here with `ring`, which Wardian already carries for TLS, so there is no AWS SDK.

use crate::ports::llm::{BedrockAuth, Llm, LlmAuth, LlmError, Tier};
use ring::{digest, hmac};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The default models, as Bedrock cross-region inference profiles for the US. Override them with
/// WARDIAN_BEDROCK_MODEL and WARDIAN_BEDROCK_QUICK_MODEL for another geography or a newer model.
const DEFAULT_MODEL: &str = "us.anthropic.claude-sonnet-4-5-20250929-v1:0";
const QUICK_MODEL: &str = "us.anthropic.claude-haiku-4-5-20251001-v1:0";
const ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";
const SERVICE: &str = "bedrock";

pub struct Bedrock {
    /// WARDIAN_BEDROCK_BASE_URL, for tests; otherwise the region's runtime endpoint.
    base: Option<String>,
    main: String,
    quick: String,
}

impl Bedrock {
    pub fn new(base: Option<String>, main: Option<String>, quick: Option<String>) -> Bedrock {
        Bedrock {
            base: base.map(|b| b.trim_end_matches('/').to_string()),
            main: main.unwrap_or_else(|| DEFAULT_MODEL.into()),
            quick: quick.unwrap_or_else(|| QUICK_MODEL.into()),
        }
    }

    fn base(&self, region: &str) -> String {
        self.base.clone().unwrap_or_else(|| format!("https://bedrock-runtime.{region}.amazonaws.com"))
    }

    /// POST /model/<id>/invoke with `body` (the Messages body without "model").
    fn invoke(&self, auth: &LlmAuth, model: &str, body: &Value) -> Result<Value, LlmError> {
        let LlmAuth::Bedrock { region, auth } = auth else {
            return Err(LlmError::Transport("these are not Amazon Bedrock credentials".into()));
        };
        let path = format!("/model/{}/invoke", uri_encode(model, false));
        let url = format!("{}{path}", self.base(region));
        let payload = serde_json::to_vec(body).map_err(|e| LlmError::Unreadable(e.to_string()))?;
        let mut req = ureq::AgentBuilder::new().timeout(Duration::from_secs(600)).build().post(&url).set("Content-Type", "application/json").set("Accept", "application/json");
        match auth {
            BedrockAuth::ApiKey(token) => req = req.set("Authorization", &format!("Bearer {token}")),
            BedrockAuth::AccessKeys { id, secret, session } => {
                let host = host_of(&url);
                let amz_date = amz_date(SystemTime::now());
                let mut headers = vec![("content-type", "application/json".to_string()), ("host", host), ("x-amz-date", amz_date.clone())];
                if !session.is_empty() {
                    headers.push(("x-amz-security-token", session.clone()));
                }
                let signed = sign(&Request { method: "POST", path: &path, query: "", headers: &headers, payload: &payload }, &Scope { amz_date: &amz_date, region, service: SERVICE }, id, secret);
                req = req.set("X-Amz-Date", &amz_date).set("Authorization", &signed);
                if !session.is_empty() {
                    req = req.set("X-Amz-Security-Token", session);
                }
            }
        }
        let resp = req.send_bytes(&payload).map_err(|e| failure(e, region, model))?;
        resp.into_json().map_err(|e| LlmError::Unreadable(e.to_string()))
    }
}

/// Bedrock's errors, in words the person in Settings can act on.
fn failure(e: ureq::Error, region: &str, model: &str) -> LlmError {
    match e {
        ureq::Error::Status(code, resp) => {
            let kind = resp.header("x-amzn-ErrorType").unwrap_or("").split(':').next().unwrap_or("").to_string();
            let body: Value = resp.into_json().unwrap_or_default();
            let msg = body["message"].as_str().or_else(|| body["Message"].as_str()).unwrap_or("").to_string();
            let msg = match (code, kind.as_str()) {
                (403, _) if msg.contains("not authorized") || msg.contains("security token") || msg.contains("signature") => {
                    format!("AWS refused the sign-in: {msg}")
                }
                (403, _) => format!("this account cannot use {model} in {region}: enable it in the Bedrock console under Model access, or choose another region or model ({msg})"),
                (_, k) if !k.is_empty() && !msg.is_empty() => format!("{k}: {msg}"),
                _ => msg,
            };
            LlmError::Status(code, msg)
        }
        ureq::Error::Transport(t) => LlmError::Transport(t.message().map(String::from).unwrap_or_else(|| t.kind().to_string())),
    }
}

impl Llm for Bedrock {
    fn model(&self, tier: Tier) -> String {
        match tier {
            Tier::Main => self.main.clone(),
            Tier::Quick => self.quick.clone(),
        }
    }

    fn messages(&self, auth: &LlmAuth, body: &Value) -> Result<Value, LlmError> {
        let mut body = body.clone();
        let model = body.as_object_mut().and_then(|o| o.remove("model")).and_then(|m| m.as_str().map(String::from)).unwrap_or_else(|| self.main.clone());
        body["anthropic_version"] = json!(ANTHROPIC_VERSION);
        self.invoke(auth, &model, &body)
    }

    /// One tiny request to the quick model: it proves the region, the sign-in and the model access.
    fn test_key(&self, auth: &LlmAuth) -> Result<(), LlmError> {
        let body = json!({ "anthropic_version": ANTHROPIC_VERSION, "max_tokens": 1, "messages": [{ "role": "user", "content": "Hi" }] });
        self.invoke(auth, &self.quick, &body).map(drop)
    }
}

// ---------- Signature Version 4 ----------

struct Request<'a> {
    method: &'a str,
    /// The path as sent, already URI-encoded once.
    path: &'a str,
    /// The canonical query string (empty for Bedrock).
    query: &'a str,
    /// Lower-case names; host included.
    headers: &'a [(&'a str, String)],
    payload: &'a [u8],
}

struct Scope<'a> {
    amz_date: &'a str,
    region: &'a str,
    service: &'a str,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, data).as_ref())
}

fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data.as_bytes()).as_ref().to_vec()
}

/// AWS's URI encoding: every byte except A–Z a–z 0–9 - _ . ~ is %XX; '/' too unless `keep_slash`.
fn uri_encode(s: &str, keep_slash: bool) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b'/' if keep_slash => "/".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The signing key: HMAC chain over the date, the region, the service and "aws4_request".
fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date);
    let k_region = hmac_sha256(&k_date, region);
    let k_service = hmac_sha256(&k_region, service);
    hmac_sha256(&k_service, "aws4_request")
}

/// The Authorization header for `req`. Services other than S3 encode the path a second time in
/// the canonical request, so a model id's ':' (sent as %3A) is signed as %253A.
fn sign(req: &Request, scope: &Scope, key_id: &str, secret: &str) -> String {
    let mut headers: Vec<(String, String)> = req.headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string())).collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = headers.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(";");
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        req.method,
        uri_encode(req.path, true),
        req.query,
        sha256_hex(req.payload)
    );
    let date = &scope.amz_date[..8];
    let credential_scope = format!("{date}/{}/{}/aws4_request", scope.region, scope.service);
    let to_sign = format!("AWS4-HMAC-SHA256\n{}\n{credential_scope}\n{}", scope.amz_date, sha256_hex(canonical.as_bytes()));
    let signature = hex(&hmac_sha256(&signing_key(secret, date, scope.region, scope.service), &to_sign));
    format!("AWS4-HMAC-SHA256 Credential={key_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}")
}

/// host[:port] of a URL, as the request's Host header carries it.
fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next().unwrap_or(rest);
    let default_port = if url.starts_with("https://") { ":443" } else { ":80" };
    host.strip_suffix(default_port).unwrap_or(host).to_string()
}

/// YYYYMMDD'T'HHMMSS'Z' in UTC.
fn amz_date(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z", rem / 3_600, (rem % 3_600) / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    // AWS's documented example credentials (Signature Version 4 examples).
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

    #[test]
    fn signing_key_matches_aws_example() {
        assert_eq!(hex(&signing_key(SECRET, "20150830", "us-east-1", "iam")), "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9");
    }

    #[test]
    fn signature_matches_aws_iam_example() {
        let headers = [
            ("content-type", "application/x-www-form-urlencoded; charset=utf-8".to_string()),
            ("host", "iam.amazonaws.com".to_string()),
            ("x-amz-date", "20150830T123600Z".to_string()),
        ];
        let req = Request { method: "GET", path: "/", query: "Action=ListUsers&Version=2010-05-08", headers: &headers, payload: b"" };
        let auth = sign(&req, &Scope { amz_date: "20150830T123600Z", region: "us-east-1", service: "iam" }, "AKIDEXAMPLE", SECRET);
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, SignedHeaders=content-type;host;x-amz-date, Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn model_ids_are_encoded_in_the_path_and_twice_in_the_signature() {
        assert_eq!(uri_encode("us.anthropic.claude-x-v1:0", false), "us.anthropic.claude-x-v1%3A0");
        assert_eq!(uri_encode("/model/a%3A0/invoke", true), "/model/a%253A0/invoke");
    }

    #[test]
    fn dates_and_hosts() {
        assert_eq!(amz_date(UNIX_EPOCH + Duration::from_secs(1_440_938_160)), "20150830T123600Z");
        assert_eq!(amz_date(UNIX_EPOCH + Duration::from_secs(951_782_400)), "20000229T000000Z");
        assert_eq!(host_of("https://bedrock-runtime.us-east-1.amazonaws.com/model/x/invoke"), "bedrock-runtime.us-east-1.amazonaws.com");
        assert_eq!(host_of("http://127.0.0.1:18192/model/x/invoke"), "127.0.0.1:18192");
    }
}
