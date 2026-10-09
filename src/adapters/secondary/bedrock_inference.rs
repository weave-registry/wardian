//! The Claude port over Amazon Bedrock (ADR-2610071106): InvokeModel with the Anthropic Messages
//! body. This file and the Anthropic adapter are the only places that name models.
//!
//! Bedrock takes the model in the URL, not the body, and wants `anthropic_version`. It signs in
//! with a Bedrock API key (a bearer token) or AWS access keys signed with Signature Version 4,
//! done here with `ring`, which Wardian already carries for TLS, so there is no AWS SDK. With an
//! AWS profile (ADR-2610091530) it asks the credentials port for keys before each request, and
//! signs with them the same way.

use crate::ports::calendar::Utc;
use crate::ports::llm::{AwsCredentials, BedrockAuth, Llm, LlmAuth, LlmError, Tier};
use ring::{digest, hmac};
use serde_json::{json, Value};
use std::sync::Arc;
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
    /// The keys of AWS profiles.
    profiles: Arc<dyn AwsCredentials>,
}

impl Bedrock {
    pub fn new(base: Option<String>, main: Option<String>, quick: Option<String>, profiles: Arc<dyn AwsCredentials>) -> Bedrock {
        Bedrock {
            base: base.map(|b| b.trim_end_matches('/').to_string()),
            main: main.unwrap_or_else(|| DEFAULT_MODEL.into()),
            quick: quick.unwrap_or_else(|| QUICK_MODEL.into()),
            profiles,
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
        let fetched;
        let keys = match auth {
            BedrockAuth::ApiKey(token) => {
                req = req.set("Authorization", &format!("Bearer {token}"));
                None
            }
            BedrockAuth::AccessKeys { id, secret, session } => Some((id, secret, session)),
            BedrockAuth::Profile { name, cli } => {
                fetched = self.profiles.keys(name, cli).map_err(LlmError::SignIn)?;
                Some((&fetched.id, &fetched.secret, &fetched.session))
            }
        };
        if let Some((id, secret, session)) = keys {
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
        let resp = req.send_bytes(&payload).map_err(|e| failure(e, region, model));
        // Keys AWS refused (expired early, or revoked) are fetched again next time.
        if let (Err(LlmError::Status(403, _)), BedrockAuth::Profile { name, .. }) = (&resp, auth) {
            self.profiles.forget(name);
        }
        let resp = resp?;
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

    /// One tiny request to `model`: it proves the region, the sign-in and the model access.
    fn test_key(&self, auth: &LlmAuth, model: &str) -> Result<(), LlmError> {
        let body = json!({ "anthropic_version": ANTHROPIC_VERSION, "max_tokens": 1, "messages": [{ "role": "user", "content": "Hi" }] });
        self.invoke(auth, model, &body).map(drop)
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
    Utc::from_unix(t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)).compact()
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

    /// A stand-in for Bedrock that answers every request with `status`, `error_type` and `message`,
    /// and records the paths asked for. Returns its address and the paths.
    fn fake_bedrock(status: u16, error_type: &'static str, message: &'static str) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let paths = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = std::sync::Arc::clone(&paths);
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let mut reader = BufReader::new(conn.try_clone().unwrap());
                let (mut first, mut len) = (String::new(), 0);
                reader.read_line(&mut first).unwrap_or(0);
                seen.lock().unwrap().push(first.split(' ').nth(1).unwrap_or("").to_string());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap_or(0);
                    }
                }
                let _ = reader.read_exact(&mut vec![0; len]);
                let body = json!({ "message": message }).to_string();
                let _ = write!(&conn, "HTTP/1.1 {status} X\r\nx-amzn-ErrorType: {error_type}:http://internal\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (format!("http://{addr}"), paths)
    }

    /// Profiles that give the keys `id`/`secret`/`session`, counting fetches and forgets.
    struct Keys {
        fetched: std::sync::Mutex<Vec<String>>,
        fail: Option<&'static str>,
    }
    impl AwsCredentials for Keys {
        fn keys(&self, profile: &str, cli: &str) -> Result<crate::ports::llm::AwsKeys, String> {
            self.fetched.lock().unwrap().push(format!("keys {profile} {cli}"));
            match self.fail {
                Some(why) => Err(why.to_string()),
                None => Ok(crate::ports::llm::AwsKeys { id: "ASIAPROFILE".into(), secret: "profile-secret".into(), session: "profile-token".into() }),
            }
        }
        fn forget(&self, profile: &str) {
            self.fetched.lock().unwrap().push(format!("forget {profile}"));
        }
    }

    fn no_profiles() -> Arc<dyn AwsCredentials> {
        Arc::new(Keys { fetched: Default::default(), fail: Some("no profiles here") })
    }

    /// ADR-2610091530: a profile's keys are asked for at each request and signed with SigV4,
    /// session token included; a refusal drops them; a profile without keys says why, unchanged.
    #[test]
    fn bedrock_profile_keys_sign_the_request() {
        let (base, _) = fake_bedrock(403, "ExpiredTokenException", "The security token included in the request is expired");
        let keys = Arc::new(Keys { fetched: Default::default(), fail: None });
        let b = Bedrock::new(Some(base.clone()), None, None, keys.clone());
        let auth = LlmAuth::Bedrock { region: "us-east-1".into(), auth: BedrockAuth::Profile { name: "work".into(), cli: "/x/aws".into() } };
        assert!(matches!(b.test_key(&auth, QUICK_MODEL), Err(LlmError::Status(403, _))));
        assert_eq!(*keys.fetched.lock().unwrap(), ["keys work /x/aws", "forget work"]);
        let b = Bedrock::new(Some(base), None, None, Arc::new(Keys { fetched: Default::default(), fail: Some("Run `aws sso login --profile work`, then try again") }));
        match b.test_key(&auth, QUICK_MODEL) {
            Err(LlmError::SignIn(why)) => assert_eq!(why, "Run `aws sso login --profile work`, then try again"),
            _ => panic!("expected the sign-in's own words"),
        }
    }

    fn api_key() -> LlmAuth {
        LlmAuth::Bedrock { region: "eu-west-1".into(), auth: BedrockAuth::ApiKey("k".into()) }
    }

    /// ADR-2610071106 #2 (#11): Bedrock's 403 for a model the account has not enabled says to
    /// enable it in that region; a 403 for the sign-in says the sign-in was refused.
    #[test]
    fn claim_a_403_says_to_enable_the_model_in_the_region() {
        let (base, _) = fake_bedrock(403, "AccessDeniedException", "You don't have access to the model with the specified model ID.");
        let b = Bedrock::new(Some(base), Some("my.model-v1:0".into()), None, no_profiles());
        match b.messages(&api_key(), &json!({ "max_tokens": 1, "messages": [] })) {
            Err(LlmError::Status(403, msg)) => {
                assert!(msg.contains("cannot use my.model-v1:0 in eu-west-1") && msg.contains("enable it in the Bedrock console under Model access"), "{msg}");
            }
            _ => panic!("expected a 403"),
        }
        let (base, _) = fake_bedrock(403, "UnrecognizedClientException", "The security token included in the request is invalid.");
        match Bedrock::new(Some(base), None, None, no_profiles()).test_key(&api_key(), QUICK_MODEL) {
            Err(LlmError::Status(403, msg)) => assert!(msg.starts_with("AWS refused the sign-in"), "{msg}"),
            _ => panic!("expected a 403"),
        }
    }

    /// ADR-2610071106 #2 (#12): the default models are US inference profiles, and
    /// WARDIAN_BEDROCK_MODEL and WARDIAN_BEDROCK_QUICK_MODEL (passed in here by main.rs) replace them,
    /// in the request's URL too.
    #[test]
    fn claim_the_models_can_be_overridden() {
        let defaults = Bedrock::new(None, None, None, no_profiles());
        assert_eq!((defaults.model(Tier::Main), defaults.model(Tier::Quick)), (DEFAULT_MODEL.to_string(), QUICK_MODEL.to_string()));
        assert!(DEFAULT_MODEL.starts_with("us.anthropic.") && QUICK_MODEL.starts_with("us.anthropic."));
        let (base, paths) = fake_bedrock(400, "ValidationException", "stop here");
        let b = Bedrock::new(Some(base), Some("eu.anthropic.main-v1:0".into()), Some("eu.anthropic.quick-v1:0".into()), no_profiles());
        assert_eq!((b.model(Tier::Main), b.model(Tier::Quick)), ("eu.anthropic.main-v1:0".to_string(), "eu.anthropic.quick-v1:0".to_string()));
        let _ = b.messages(&api_key(), &json!({ "max_tokens": 1, "messages": [] }));
        let _ = b.test_key(&api_key(), &b.model(Tier::Quick));
        assert_eq!(*paths.lock().unwrap(), ["/model/eu.anthropic.main-v1%3A0/invoke", "/model/eu.anthropic.quick-v1%3A0/invoke"]);
    }

    #[test]
    fn dates_and_hosts() {
        assert_eq!(amz_date(UNIX_EPOCH + Duration::from_secs(1_440_938_160)), "20150830T123600Z");
        assert_eq!(amz_date(UNIX_EPOCH + Duration::from_secs(951_782_400)), "20000229T000000Z");
        assert_eq!(host_of("https://bedrock-runtime.us-east-1.amazonaws.com/model/x/invoke"), "bedrock-runtime.us-east-1.amazonaws.com");
        assert_eq!(host_of("http://127.0.0.1:18192/model/x/invoke"), "127.0.0.1:18192");
    }
}
