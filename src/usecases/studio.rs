//! "Make an app": a chat in which Claude writes an app's files.
//!
//! Claude never touches the apps folder directly. It works on a copy of the
//! package held in memory, through a few tools (list, read, write, delete,
//! check, finish). Only `finish` saves, and only when `wardian check` passes:
//! the files go to a hidden staging folder, are checked there, and then
//! replace the app in one rename. The version they replace goes to the trash,
//! so every AI change can be undone with Restore.
//!
//! Claude cannot compile anything here, so AI-made apps are plain
//! JavaScript, HTML and CSS: a page app (Wardian adds an empty app.wasm, the
//! marker that makes a folder an app) or a suite.

use super::ai_tools as ai_tools_uc;
use super::aws_profiles::AwsProfiles;
use super::catalog::Hub;
use super::check::Checker;
use super::keys::{KeyChecks, KeyEntry, KeyOwner};
use super::usage::Meter;
use super::workshop::{self, Files};
use crate::domain::agent::{AgentSettings, BUILDER, LIMITS};
use crate::domain::aws_profile;
use crate::domain::package::{safe_segment, SKIP_DIRS};
use crate::domain::studio::{free_name, json_in, Session, EMPTY_WASM};
use crate::ports::{
    assets::Assets,
    clock::{Clock, Tasks},
    llm::{BedrockAuth, Llm, LlmAuth, LlmError, Tier},
    secrets::Secrets,
    service::Builder,
    storage::FileSystem,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

// The steps, tokens and chats of Make an app, and the tokens of a `claude:sample` reply, are
// settings (ADR-2610081500, `domain::agent`).
const MAX_MESSAGE_CHARS: usize = 20_000;
/// The most an app may send in one `claude:sample` prompt.
const MAX_SAMPLE_CHARS: usize = 60_000;

/// A failed model call, in words.
fn api_error(e: LlmError) -> String {
    match e {
        LlmError::Status(401, _) => "the API key was refused (401). Check it in Settings.".into(),
        LlmError::Status(code, msg) if msg.is_empty() => format!("the API answered HTTP {code}"),
        LlmError::Status(code, msg) => format!("the API answered HTTP {code}: {msg}"),
        LlmError::Transport(why) => format!("cannot reach the API: {why}"),
        LlmError::Unreadable(why) => format!("unreadable reply from the API: {why}"),
        LlmError::SignIn(why) => why,
    }
}

/// The model, the key, and what the build loop needs to call it.
struct Api {
    llm: Arc<dyn Llm>,
    auth: LlmAuth,
    assets: Arc<dyn Assets>,
    /// The models for each tier, chosen in Settings or the adapter's defaults.
    model: String,
    quick_model: String,
    /// Model requests in one turn of the chat, and tokens in one reply.
    max_steps: usize,
    max_tokens: u64,
    /// Who pays (a package, or `BUILDER`), the meter it is counted on, and its daily cap.
    payer: String,
    meter: Arc<Meter>,
    cap: Option<u64>,
    /// Where a refused key is recorded, under the provider's id.
    checks: Arc<KeyChecks>,
    provider_id: &'static str,
    clock: Arc<dyn Clock>,
}

impl Api {
    /// One model request of the build loop. Retries rate limits and overloads a few times.
    fn messages(&self, messages: &[Value], cancel: &AtomicBool) -> Result<Value, String> {
        let mut messages = messages.to_vec();
        // Cache the conversation so far: the next request reuses it.
        if let Some(Value::Array(blocks)) = messages.last_mut().map(|m| &mut m["content"]) {
            if let Some(Value::Object(last)) = blocks.last_mut() {
                last.insert("cache_control".into(), json!({ "type": "ephemeral" }));
            }
        }
        let body = json!({
            "model": self.model,
            "max_tokens": self.max_tokens,
            "system": workshop::system(&*self.assets),
            "tools": workshop::tools(),
            "messages": messages,
        });
        self.send(&body, cancel)
    }

    /// The skills agent (ADR-2610131000): research_ai_tools as its own request, with its own
    /// prompt and no tools, on Make an app's meter and cap. A failure is the tool's error, for the
    /// builder to work around, not the end of the turn; only a stop ends it.
    fn skills(&self, input: &Value, files: &Files, cancel: &AtomicBool) -> Result<workshop::Outcome, String> {
        let mut body = match workshop::skills_request(&*self.assets, input, files) {
            Ok(b) => b,
            Err(e) => return Ok(workshop::Outcome { text: e, is_error: true, ..Default::default() }),
        };
        body["model"] = json!(self.model);
        body["max_tokens"] = json!(self.max_tokens.min(workshop::SKILLS_MAX_TOKENS));
        match self.send(&body, cancel) {
            Ok(reply) => Ok(workshop::skills_outcome(&reply)),
            Err(e) if e == "stopped" => Err(e),
            Err(e) => Ok(workshop::Outcome {
                text: format!("The skills agent could not answer ({e}). Design the AI yourself from the agent component's guide and rule (11) of the checklist."),
                is_error: true,
                shown: Some("the skills agent could not answer".into()),
                saved: None,
            }),
        }
    }

    /// Sends one request of Make an app, under its daily cap, retrying rate limits and overloads.
    fn send(&self, body: &Value, cancel: &AtomicBool) -> Result<Value, String> {
        if let Some(cap) = self.cap.filter(|&c| self.meter.used_today(&self.payer) >= c) {
            return Err(format!("Make an app used its {cap} tokens for today. An admin can raise the cap in Settings → Claude."));
        }
        let mut wait = 2;
        loop {
            match self.call(body) {
                Ok(v) => return Ok(v),
                Err(LlmError::Status(code, _)) if matches!(code, 429 | 500 | 502 | 503 | 529) && wait <= 16 => {
                    for _ in 0..wait * 10 {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("stopped".into());
                        }
                        self.clock.sleep(Duration::from_millis(100));
                    }
                    wait *= 2;
                }
                Err(e) => return Err(api_error(e)),
            }
        }
    }

    /// One question from an app (`claude:sample`), no tools. Errors start with a
    /// code the app can act on ("rate_limited: …"), the same codes the Claude
    /// viewer uses.
    fn sample(&self, prompt: &str, quick: bool) -> Result<Value, String> {
        if let Some(cap) = self.cap.filter(|&c| self.meter.used_today(&self.payer) >= c) {
            return Err(format!("over_budget: this app used its {cap} tokens of Claude for today; an admin can raise its cap in Settings → Claude"));
        }
        let model = if quick { self.quick_model.clone() } else { self.model.clone() };
        let max_tokens = self.max_tokens;
        let body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let mut wait = 2;
        let resp: Value = loop {
            match self.call(&body) {
                Ok(r) => break r,
                Err(LlmError::Status(code, _)) if matches!(code, 500 | 502 | 503 | 529) && wait <= 8 => {
                    self.clock.sleep(Duration::from_secs(wait));
                    wait *= 2;
                }
                Err(LlmError::Status(429, _)) => return Err("rate_limited: Claude's rate limit was reached".into()),
                Err(LlmError::Status(401, _)) => return Err("not_granted: Claude refused the saved credentials; check Settings".into()),
                Err(e) => return Err(format!("error: {}", api_error(e))),
            }
        };
        let text: String = resp["content"]
            .as_array()
            .map(|blocks| blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join(""))
            .unwrap_or_default();
        let stop = resp["stop_reason"].as_str().unwrap_or("");
        if stop == "refusal" {
            return Err("refused: Claude declined this request".into());
        }
        Ok(json!({ "text": text, "truncated": stop == "max_tokens", "model": model }))
    }

    /// One request: its tokens go on the meter, and a refused key is recorded for Settings.
    fn call(&self, body: &Value) -> Result<Value, LlmError> {
        let out = self.llm.messages(&self.auth, body);
        match &out {
            Ok(reply) => self.meter.record(&self.payer, reply),
            Err(LlmError::Status(code @ (401 | 403), msg)) => {
                let said = format!("refused while {} used it (HTTP {code}) {msg}", self.payer);
                self.checks.record::<()>(self.provider_id, &Err(said.trim().to_string()));
            }
            Err(LlmError::SignIn(why)) => self.checks.record::<()>(self.provider_id, &Err(why.clone())),
            Err(_) => {}
        }
        out
    }
}

/// Where Claude is reached (ADR-2610071106).
#[derive(Clone, Copy, PartialEq)]
enum Provider {
    Anthropic,
    Bedrock,
}

impl Provider {
    /// Its name in `agent.json` and its key's id on the key list.
    fn id(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::Bedrock => "bedrock",
        }
    }
}

/// Amazon Bedrock: the region and how to sign in.
#[derive(Clone)]
pub struct BedrockSettings {
    pub region: String,
    pub auth: BedrockAuth,
}

impl BedrockSettings {
    fn auth_kind(&self) -> &'static str {
        match self.auth {
            BedrockAuth::ApiKey(_) => "api-key",
            BedrockAuth::AccessKeys { .. } => "access-keys",
            BedrockAuth::Profile { .. } => "profile",
        }
    }

    /// The AWS profile's name, for a profile; never a key.
    fn profile(&self) -> Option<&str> {
        match &self.auth {
            BedrockAuth::Profile { name, .. } => Some(name),
            _ => None,
        }
    }

    /// How it signs in, in words.
    fn auth_words(&self) -> String {
        match &self.auth {
            BedrockAuth::ApiKey(_) => "API key".into(),
            BedrockAuth::AccessKeys { .. } => "access keys".into(),
            BedrockAuth::Profile { name, .. } => format!("AWS profile {name}"),
        }
    }

    fn to_json(&self) -> Value {
        match &self.auth {
            BedrockAuth::ApiKey(t) => json!({ "region": self.region, "auth": "api-key", "token": t }),
            BedrockAuth::AccessKeys { id, secret, session } => {
                json!({ "region": self.region, "auth": "access-keys", "access_key_id": id, "secret_access_key": secret, "session_token": session })
            }
            BedrockAuth::Profile { name, cli } => json!({ "region": self.region, "auth": "profile", "profile": name, "aws_cli": cli }),
        }
    }

    /// From the settings page or bedrock.json. Fields left empty keep the values of `keep`, so the
    /// browser never needs the saved secrets to change the region. A profile's region may be
    /// empty here: the caller takes the profile's own.
    fn from_json(v: &Value, keep: Option<&BedrockSettings>) -> Result<BedrockSettings, String> {
        let s = |k: &str| v[k].as_str().unwrap_or("").trim().to_string();
        let region = s("region");
        let profile = v["auth"].as_str() == Some("profile");
        if (region.is_empty() && !profile) || region.len() > 40 || !region.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err("give the AWS region, such as us-east-1".into());
        }
        if !region.is_empty() {
            aws_profile::check_region(&region)?;
        }
        let field = |name: &str, kept: Option<&String>| -> Result<String, String> {
            let typed = s(name);
            let val = if typed.is_empty() { kept.cloned().unwrap_or_default() } else { typed };
            if val.len() > 4096 || !val.chars().all(|c| c.is_ascii_graphic()) {
                return Err(format!("{} does not look right", name.replace('_', " ")));
            }
            Ok(val)
        };
        let auth = match v["auth"].as_str().unwrap_or("api-key") {
            "api-key" => {
                let kept = keep.and_then(|k| if let BedrockAuth::ApiKey(t) = &k.auth { Some(t) } else { None });
                let token = field("token", kept)?;
                if token.is_empty() {
                    return Err("paste the Bedrock API key".into());
                }
                BedrockAuth::ApiKey(token)
            }
            "access-keys" => {
                let (kid, ksecret, ksession) = match keep.map(|k| &k.auth) {
                    Some(BedrockAuth::AccessKeys { id, secret, session }) => (Some(id), Some(secret), Some(session)),
                    _ => (None, None, None),
                };
                let id = field("access_key_id", kid)?;
                let secret = field("secret_access_key", ksecret)?;
                if id.is_empty() || secret.is_empty() {
                    return Err("give the access key ID and the secret access key".into());
                }
                BedrockAuth::AccessKeys { id, secret, session: field("session_token", ksession)? }
            }
            "profile" => {
                let kept = keep.and_then(|k| k.profile());
                let typed = s("profile");
                let name = if typed.is_empty() { kept.unwrap_or("").to_string() } else { typed };
                if name.is_empty() {
                    return Err("choose the AWS profile".into());
                }
                if !aws_profile::valid_profile_name(&name) {
                    return Err("that does not look like an AWS profile name".into());
                }
                let cli = s("aws_cli");
                BedrockAuth::Profile { name, cli }
            }
            other => return Err(format!("unknown sign-in \"{other}\": use api-key, access-keys or profile")),
        };
        Ok(BedrockSettings { region, auth })
    }
}

/// The two providers, and what the environment says about them, from the composition root.
pub struct Providers {
    pub anthropic: Arc<dyn Llm>,
    pub bedrock: Arc<dyn Llm>,
    /// ANTHROPIC_API_KEY and ANTHROPIC_WORKSPACE_ID, used when Settings has none.
    pub anthropic_key: Option<String>,
    pub anthropic_workspace: Option<String>,
    /// WARDIAN_AI_PROVIDER, used when Settings has not chosen.
    pub provider: Option<String>,
    /// AWS_REGION with AWS_BEARER_TOKEN_BEDROCK or the AWS access keys, or AWS_PROFILE, used when
    /// Settings has none.
    pub bedrock_env: Option<BedrockSettings>,
    /// The AWS profiles on this machine (ADR-2610091530).
    pub profiles: Arc<AwsProfiles>,
}

/// Where the studio keeps what it holds (ADR-2610081500).
pub struct Stores {
    pub fs: Arc<dyn FileSystem>,
    pub secrets: Arc<dyn Secrets>,
    pub checks: Arc<KeyChecks>,
    pub meter: Arc<Meter>,
}

pub struct Studio {
    fs: Arc<dyn FileSystem>,
    secrets: Arc<dyn Secrets>,
    checks: Arc<KeyChecks>,
    meter: Arc<Meter>,
    agent_path: PathBuf,
    agent: Mutex<AgentSettings>,
    /// Saved secrets that are there but cannot be read, by key id.
    unreadable: Mutex<BTreeMap<&'static str, String>>,
    llm: Arc<dyn Llm>,
    bedrock_llm: Arc<dyn Llm>,
    provider_path: PathBuf,
    provider: Mutex<Provider>,
    bedrock_path: PathBuf,
    bedrock: Mutex<Option<(BedrockSettings, &'static str)>>,
    env_bedrock: Option<BedrockSettings>,
    profiles: Arc<AwsProfiles>,
    assets: Arc<dyn Assets>,
    hub: Arc<Hub>,
    checker: Arc<Checker>,
    key_path: PathBuf,
    key: Mutex<Option<(String, &'static str)>>,
    /// The key from ANTHROPIC_API_KEY, used when Settings has none.
    env_key: Option<String>,
    workspace_path: PathBuf,
    workspace: Mutex<String>,
    sessions: Mutex<HashMap<String, Arc<Mutex<Session>>>>,
    clock: Arc<dyn Clock>,
    tasks: Arc<dyn Tasks>,
}

/// What the build loop works with: the model, the apps folder and the checker.
struct Workshop<'a> {
    api: &'a Api,
    fs: &'a dyn FileSystem,
    hub: &'a Hub,
    checker: &'a Checker,
    clock: &'a dyn Clock,
}

impl Studio {
    pub fn new(stores: Stores, providers: Providers, assets: Arc<dyn Assets>, hub: Arc<Hub>, checker: Arc<Checker>, data_dir: &Path, clock: Arc<dyn Clock>, tasks: Arc<dyn Tasks>) -> Studio {
        let Stores { fs, secrets, checks, meter } = stores;
        let mut unreadable = BTreeMap::new();
        let mut secret = |id: &'static str, path: &Path| match secrets.read(path) {
            Ok(b) => b,
            Err(e) => {
                unreadable.insert(id, e);
                None
            }
        };
        let agent_path = data_dir.join("agent.json");
        let agent = match fs.read(&agent_path).map(|b| serde_json::from_slice::<Value>(&b).map_err(|e| e.to_string()).and_then(|v| AgentSettings::from_saved(&v))) {
            Some(Ok(a)) => a,
            Some(Err(e)) => {
                eprintln!("claude: {} is not valid, so the defaults apply: {e}", agent_path.display());
                AgentSettings::default()
            }
            None => AgentSettings::default(),
        };
        let Providers { anthropic: llm, bedrock: bedrock_llm, anthropic_key: env_key, anthropic_workspace: env_workspace, provider: env_provider, bedrock_env: env_bedrock, profiles } = providers;
        // Settings saved in the data folder win over the environment, as for the Anthropic key.
        let provider_path = data_dir.join("ai-provider");
        let chosen = fs.read(&provider_path).map(|b| String::from_utf8_lossy(&b).trim().to_string()).or(env_provider);
        let provider = if chosen.as_deref() == Some("bedrock") { Provider::Bedrock } else { Provider::Anthropic };
        let bedrock_path = data_dir.join("bedrock.json");
        let saved_bedrock = secret("bedrock", &bedrock_path)
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| BedrockSettings::from_json(&v, None).ok());
        let bedrock = match saved_bedrock {
            Some(b) => Some((b, "settings")),
            None => env_bedrock.clone().map(|b| (b, "environment")),
        };
        let key_path = data_dir.join("anthropic-key");
        let saved = secret("anthropic", &key_path).map(|b| String::from_utf8_lossy(&b).trim().to_string()).filter(|k| !k.is_empty());
        let key = match saved {
            Some(k) => Some((k, "settings")),
            None => env_key.clone().map(|k| (k, "environment")),
        };
        let workspace_path = data_dir.join("anthropic-workspace");
        let workspace = fs
            .read(&workspace_path)
            .map(|b| String::from_utf8_lossy(&b).trim().to_string())
            .filter(|w| !w.is_empty())
            .or(env_workspace)
            .unwrap_or_default();
        Studio {
            fs,
            secrets,
            checks,
            meter,
            agent_path,
            agent: Mutex::new(agent),
            unreadable: Mutex::new(unreadable),
            llm,
            bedrock_llm,
            provider_path,
            provider: Mutex::new(provider),
            bedrock_path,
            bedrock: Mutex::new(bedrock),
            env_bedrock,
            profiles,
            assets,
            hub,
            checker,
            key_path,
            key: Mutex::new(key),
            env_key,
            workspace_path,
            workspace: Mutex::new(workspace),
            sessions: Mutex::new(HashMap::new()),
            clock,
            tasks,
        }
    }

    /// The chosen provider, with its credentials, models and limits, for `payer`: a package, or
    /// `BUILDER` for Make an app.
    fn api(&self, payer: &str) -> Result<Api, String> {
        let provider = *self.provider.lock().unwrap();
        let (llm, auth) = if provider == Provider::Bedrock {
            let b = self.bedrock.lock().unwrap().as_ref().map(|(b, _)| b.clone());
            let b = b.ok_or("Amazon Bedrock is not set up yet: Settings → Claude")?;
            (Arc::clone(&self.bedrock_llm), LlmAuth::Bedrock { region: b.region, auth: b.auth })
        } else {
            let key = self.key.lock().unwrap().as_ref().map(|(k, _)| k.clone());
            let key = key.ok_or("no Anthropic API key yet: add one in Settings → Claude")?;
            (Arc::clone(&self.llm), LlmAuth::Anthropic { key, workspace: self.workspace.lock().unwrap().clone() })
        };
        let agent = self.agent.lock().unwrap();
        let building = payer == BUILDER;
        let id = provider.id();
        Ok(Api {
            model: agent.model(id, false).map(String::from).unwrap_or_else(|| llm.model(Tier::Main)),
            quick_model: agent.model(id, true).map(String::from).unwrap_or_else(|| llm.model(Tier::Quick)),
            max_steps: agent.limit("max_steps") as usize,
            max_tokens: agent.limit(if building { "max_tokens" } else { "sample_max_tokens" }),
            cap: if building { agent.build_cap() } else { agent.sample_cap(payer) },
            payer: payer.to_string(),
            meter: Arc::clone(&self.meter),
            checks: Arc::clone(&self.checks),
            provider_id: id,
            clock: Arc::clone(&self.clock),
            llm,
            auth,
            assets: Arc::clone(&self.assets),
        })
    }

    /// The model a provider uses for a tier: the one chosen in Settings, or the adapter's default.
    fn model_of(&self, provider: Provider, tier: Tier) -> String {
        let llm = if provider == Provider::Bedrock { &self.bedrock_llm } else { &self.llm };
        let chosen = self.agent.lock().unwrap().model(provider.id(), matches!(tier, Tier::Quick)).map(String::from);
        chosen.unwrap_or_else(|| llm.model(tier))
    }

    /// Tests an Anthropic key with the main model, and records the result.
    fn test_anthropic(&self, key: &str, workspace: &str) -> Result<(), String> {
        let auth = LlmAuth::Anthropic { key: key.into(), workspace: workspace.into() };
        let out = self.llm.test_key(&auth, &self.model_of(Provider::Anthropic, Tier::Main)).map_err(api_error);
        self.checks.record("anthropic", &out);
        out
    }

    /// Tests Bedrock settings with one tiny request to the quick model, and records the result.
    fn test_bedrock(&self, b: &BedrockSettings) -> Result<(), String> {
        let auth = LlmAuth::Bedrock { region: b.region.clone(), auth: b.auth.clone() };
        let out = self.bedrock_llm.test_key(&auth, &self.model_of(Provider::Bedrock, Tier::Quick)).map_err(api_error);
        self.checks.record("bedrock", &out);
        out
    }

    fn forget_anthropic(&self) {
        self.secrets.remove(&self.key_path);
        self.unreadable.lock().unwrap().remove("anthropic");
        *self.key.lock().unwrap() = self.env_key.clone().map(|k| (k, "environment"));
    }

    fn forget_bedrock(&self) {
        self.secrets.remove(&self.bedrock_path);
        self.unreadable.lock().unwrap().remove("bedrock");
        *self.bedrock.lock().unwrap() = self.env_bedrock.clone().map(|b| (b, "environment"));
    }

    /// Claude's settings (ADR-2610081500): the saved ones, every limit's bounds, and the models
    /// each provider uses now.
    pub fn agent(&self) -> Value {
        let settings = self.agent.lock().unwrap().to_json();
        let limits: Vec<Value> = LIMITS.iter().map(|l| json!({ "name": l.name, "default": l.default, "min": l.min, "max": l.max })).collect();
        let in_use = |p: Provider| json!({ "main": self.model_of(p, Tier::Main), "quick": self.model_of(p, Tier::Quick),
                                            "default_main": self.llm_of(p).model(Tier::Main), "default_quick": self.llm_of(p).model(Tier::Quick) });
        json!({ "settings": settings, "limits": limits,
                "models": { "anthropic": in_use(Provider::Anthropic), "bedrock": in_use(Provider::Bedrock) } })
    }

    /// Applies a patch of Claude's settings. A model that changes is tested first, with the
    /// provider's saved credentials, so a typo never replaces a working model; without
    /// credentials it cannot be tested, and the answer says so.
    pub fn set_agent(&self, patch: &Value) -> Result<Value, String> {
        let current = self.agent.lock().unwrap().clone();
        let next = current.apply(patch)?;
        let mut untested = Vec::new();
        for provider in [Provider::Anthropic, Provider::Bedrock] {
            for quick in [false, true] {
                let Some(model) = next.model(provider.id(), quick).filter(|m| current.model(provider.id(), quick) != Some(m)) else { continue };
                match self.credentials(provider) {
                    Some(auth) => self.llm_of(provider).test_key(&auth, model).map_err(|e| format!("{model}: {}", api_error(e)))?,
                    None => untested.push(model.to_string()),
                }
            }
        }
        let bytes = serde_json::to_vec_pretty(&next.to_json()).map_err(|e| e.to_string())?;
        self.fs.write_private(&self.agent_path, &bytes).map_err(|e| format!("saving Claude's settings: {e}"))?;
        *self.agent.lock().unwrap() = next;
        let mut out = self.agent();
        out["untested"] = json!(untested);
        Ok(out)
    }

    /// The tokens used, by day and payer, with today's cap of each payer.
    pub fn usage(&self) -> Value {
        let mut out = self.meter.report();
        let agent = self.agent.lock().unwrap();
        let payers: Vec<String> = out["totals"].as_object().map(|t| t.keys().cloned().collect()).unwrap_or_default();
        let caps: serde_json::Map<String, Value> = payers
            .iter()
            .map(|p| (p.clone(), json!(if p == BUILDER { agent.build_cap() } else { agent.sample_cap(p) })))
            .collect();
        out["caps"] = Value::Object(caps);
        out["sample_daily_tokens"] = json!(agent.limit("sample_daily_tokens"));
        out["build_daily_tokens"] = json!(agent.limit("build_daily_tokens"));
        out
    }

    fn llm_of(&self, provider: Provider) -> &Arc<dyn Llm> {
        if provider == Provider::Bedrock { &self.bedrock_llm } else { &self.llm }
    }

    /// A provider's saved credentials, if it has any.
    fn credentials(&self, provider: Provider) -> Option<LlmAuth> {
        match provider {
            Provider::Anthropic => {
                let key = self.key.lock().unwrap().as_ref().map(|(k, _)| k.clone())?;
                Some(LlmAuth::Anthropic { key, workspace: self.workspace.lock().unwrap().clone() })
            }
            Provider::Bedrock => self.bedrock.lock().unwrap().as_ref().map(|(b, _)| LlmAuth::Bedrock { region: b.region.clone(), auth: b.auth.clone() }),
        }
    }

    /// What Settings shows. Never a key, token or secret.
    pub fn status(&self) -> Value {
        let provider = *self.provider.lock().unwrap();
        let key = self.key.lock().unwrap();
        let ws = self.workspace.lock().unwrap();
        let bedrock = self.bedrock.lock().unwrap();
        let bedrock_info = bedrock.as_ref().map(|(b, from)| json!({ "region": b.region, "auth": b.auth_kind(), "profile": b.profile(), "from": from }));
        let ready = match provider {
            Provider::Anthropic => key.is_some(),
            Provider::Bedrock => bedrock.is_some(),
        };
        let model = self.model_of(provider, Tier::Main);
        json!({ "ready": ready, "provider": if provider == Provider::Bedrock { "bedrock" } else { "anthropic" }, "model": model,
                "key_from": key.as_ref().map(|(_, from)| *from),
                "workspace": if ws.is_empty() { Value::Null } else { json!(*ws) },
                "anthropic": { "ready": key.is_some(), "model": self.model_of(Provider::Anthropic, Tier::Main), "quick_model": self.model_of(Provider::Anthropic, Tier::Quick) },
                "bedrock": { "ready": bedrock.is_some(), "settings": bedrock_info, "model": self.model_of(Provider::Bedrock, Tier::Main), "quick_model": self.model_of(Provider::Bedrock, Tier::Quick) } })
    }

    /// Chooses the provider (ADR-2610071106). For Bedrock, tests the settings before saving them,
    /// so a bad setting never replaces a good one. Fields left empty keep the saved values.
    /// `{provider: "anthropic"}` switches back; `{provider: "bedrock", forget: true}` removes the
    /// saved Bedrock settings.
    pub fn set_provider(&self, body: &Value) -> Result<Value, String> {
        match body["provider"].as_str() {
            Some("anthropic") => {
                self.fs.write_private(&self.provider_path, b"anthropic").map_err(|e| format!("saving the choice: {e}"))?;
                *self.provider.lock().unwrap() = Provider::Anthropic;
            }
            Some("bedrock") if body["forget"].as_bool() == Some(true) => self.forget_bedrock(),
            Some("bedrock") => {
                let keep = self.bedrock.lock().unwrap().as_ref().map(|(b, _)| b.clone());
                let mut settings = BedrockSettings::from_json(body, keep.as_ref())?;
                self.with_profile(&mut settings)?;
                self.test_bedrock(&settings)?;
                let bytes = serde_json::to_vec_pretty(&settings.to_json()).map_err(|e| e.to_string())?;
                self.secrets.write(&self.bedrock_path, &bytes).map_err(|e| format!("saving the Bedrock settings: {e}"))?;
                self.unreadable.lock().unwrap().remove("bedrock");
                self.fs.write_private(&self.provider_path, b"bedrock").map_err(|e| format!("saving the choice: {e}"))?;
                *self.bedrock.lock().unwrap() = Some((settings, "settings"));
                *self.provider.lock().unwrap() = Provider::Bedrock;
            }
            _ => return Err("provider must be \"anthropic\" or \"bedrock\"".into()),
        }
        Ok(self.status())
    }

    /// For an AWS profile (ADR-2610091530): checks the profile is there, takes its region when
    /// none was chosen, and records where the AWS CLI is now, so a Wardian started as a service,
    /// with a bare PATH, finds it.
    fn with_profile(&self, settings: &mut BedrockSettings) -> Result<(), String> {
        let BedrockAuth::Profile { name, cli } = &mut settings.auth else { return Ok(()) };
        let region = self.profiles.region_of(name)?;
        if settings.region.is_empty() {
            settings.region = region.ok_or_else(|| aws_profile::no_region(name))?;
            aws_profile::check_region(&settings.region).map_err(|e| format!("AWS profile \"{name}\": {e}"))?;
        }
        *cli = self.profiles.find_cli().unwrap_or_default();
        // Keys kept from before are not trusted for a new test.
        crate::ports::llm::AwsCredentials::forget(&*self.profiles, name);
        Ok(())
    }

    /// The AWS profiles for Settings: names and regions, never keys.
    pub fn aws_profiles(&self) -> Value {
        self.profiles.list()
    }

    /// Answers one `claude:sample` request from an app. `body`: {prompt, json, tier}.
    pub fn sample(&self, body: &Value) -> Result<Value, String> {
        let package = body["package"].as_str().unwrap_or("");
        let api = self.api(package).map_err(|e| format!("not_granted: {e}"))?;
        let prompt = body["prompt"].as_str().unwrap_or("").trim();
        if prompt.is_empty() {
            return Err("error: the prompt is empty".into());
        }
        if prompt.chars().count() > MAX_SAMPLE_CHARS {
            return Err("prompt_too_large: the prompt is too long".into());
        }
        let quick = body["tier"].as_str() == Some("quick");
        if body["json"].as_bool().unwrap_or(false) {
            let ask = format!("{prompt}\n\nAnswer with the JSON only, no other words.");
            let out = api.sample(&ask, quick)?;
            let value = json_in(out["text"].as_str().unwrap_or("")).ok_or("invalid_json: the answer was not JSON")?;
            Ok(json!({ "value": value }))
        } else {
            api.sample(prompt, quick)
        }
    }

    /// A local app's files, for App settings (ADR-2610131300).
    fn local_files(&self, app: &str) -> Result<Files, String> {
        if !self.hub.serving_local() {
            return Err("App settings change apps in the local apps folder. Switch the source to Local first.".into());
        }
        if !safe_segment(app) || !self.hub.local_app_exists(app) {
            return Err(format!("no local app \"{app}\""));
        }
        Ok(load(&*self.fs, &self.hub.local_root().join(app)))
    }

    /// An app's AI skill and tools, as App settings shows them (`ai_tools::view`).
    pub fn ai_tools(&self, app: &str) -> Result<Value, String> {
        Ok(ai_tools_uc::view(&self.local_files(app)?))
    }

    /// Saves the skill and tools from App settings, checked like any save and kept in the app's
    /// history. `body`: {repository: {skill, tools}, why?}.
    pub fn save_ai_tools(&self, app: &str, body: &Value) -> Result<Value, String> {
        let mut files = self.local_files(app)?;
        ai_tools_uc::apply(&mut files, &body["repository"])?;
        let why = body["why"].as_str().map(str::trim).filter(|w| !w.is_empty()).unwrap_or("Changed the AI's skill and tools in App settings");
        let why: String = why.chars().take(300).collect();
        let (_, version) = save_package(&*self.fs, &self.hub, &self.checker, &*self.clock, &files, app, "app-settings", &why)?;
        let mut out = self.ai_tools(app)?;
        out["version"] = json!(version);
        Ok(out)
    }

    /// Improve with AI: Claude rewrites the skill and tools the editor holds, on Make an app's
    /// meter and cap. Nothing is saved. `body`: {repository, wanted}.
    pub fn improve_ai_tools(&self, app: &str, body: &Value) -> Result<Value, String> {
        let files = self.local_files(app)?;
        let mut req = ai_tools_uc::improve_request_for(&files, &body["repository"], body["wanted"].as_str().unwrap_or(""))?;
        let api = self.api(BUILDER)?;
        req["model"] = json!(api.model);
        req["max_tokens"] = json!(api.max_tokens.min(ai_tools_uc::IMPROVE_MAX_TOKENS));
        let reply = api.send(&req, &AtomicBool::new(false))?;
        ai_tools_uc::improve_outcome_for(&reply, &files)
    }

    /// Tests a key against the API before saving it, so a bad key never
    /// replaces a good one. An empty key removes the saved one.
    ///
    /// `workspace` (when given) is the workspace ID for keys that are not
    /// scoped to one workspace; an empty one removes it. With an empty key
    /// and a workspace, the saved key is tested again with the new workspace.
    pub fn set_key(&self, key: &str, workspace: Option<&str>) -> Result<Value, String> {
        let key = key.trim();
        let workspace = match workspace.map(str::trim) {
            Some(w) if w.len() > 100 || !w.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') => {
                return Err("that does not look like a workspace ID (letters, digits, '_' and '-')".into())
            }
            w => w.map(String::from),
        };
        if key.is_empty() && workspace.is_none() {
            self.forget_anthropic();
            return Ok(self.status());
        }
        let typed = !key.is_empty();
        let key = if typed {
            if key.len() > 300 || !key.chars().all(|c| c.is_ascii_graphic()) {
                return Err("that does not look like an API key".into());
            }
            key.to_string()
        } else {
            self.key.lock().unwrap().as_ref().map(|(k, _)| k.clone()).ok_or("paste the API key too")?
        };
        let ws = workspace.clone().unwrap_or_else(|| self.workspace.lock().unwrap().clone());
        self.test_anthropic(&key, &ws)?;
        if typed {
            self.secrets.write(&self.key_path, key.as_bytes()).map_err(|e| format!("saving the key: {e}"))?;
            *self.key.lock().unwrap() = Some((key, "settings"));
            self.unreadable.lock().unwrap().remove("anthropic");
        }
        if workspace.is_some() {
            if ws.is_empty() {
                self.fs.remove_file(&self.workspace_path);
            } else {
                self.fs.write_private(&self.workspace_path, ws.as_bytes()).map_err(|e| format!("saving the workspace: {e}"))?;
            }
            *self.workspace.lock().unwrap() = ws;
        }
        Ok(self.status())
    }

    /// Adds the user's message to a chat (a new one when `session` is
    /// missing) and starts Claude's turn in the background.
    pub fn send(&self, body: &Value) -> Result<Value, String> {
        let api = self.api(BUILDER)?;
        if !self.hub.serving_local() {
            return Err("AI apps are saved in the local apps folder. Switch the source to Local first.".into());
        }
        let text = body["message"].as_str().unwrap_or("").trim();
        if text.is_empty() {
            return Err("write what the app should do".into());
        }
        if text.len() > MAX_MESSAGE_CHARS {
            return Err("that message is too long".into());
        }
        let (id, session) = match body["session"].as_str().filter(|s| !s.is_empty()) {
            Some(id) => {
                let s = self.sessions.lock().unwrap().get(id).cloned();
                (id.to_string(), s.ok_or("that chat has ended (Wardian restarted); start a new one")?)
            }
            None => {
                let app = body["app"].as_str().filter(|a| !a.is_empty());
                if let Some(app) = app {
                    if !self.hub.local_app_exists(app) {
                        return Err(format!("no local app \"{app}\" to change"));
                    }
                }
                let id = format!("{:x}{:08x}", self.clock.now(), self.clock.nonce());
                let s = Arc::new(Mutex::new(Session::new(app.map(String::from), self.clock.now())));
                self.remember(&id, Arc::clone(&s));
                (id, s)
            }
        };

        let mut s = session.lock().unwrap();
        if s.busy {
            return Err("Claude is still working on the last message".into());
        }
        let mut said = text.to_string();
        if s.messages.is_empty() {
            if let Some(app) = &s.app {
                said = format!("I want to change the existing app \"{app}\".\n\n{said}");
            }
        }
        // After a save, the last message holds finish's result; a new user
        // turn must join it, since two user messages in a row are not allowed.
        match s.messages.last_mut() {
            Some(m) if m["role"] == "user" => {
                if let Some(blocks) = m["content"].as_array_mut() {
                    blocks.push(json!({ "type": "text", "text": said }));
                }
            }
            _ => s.messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": said }] })),
        }
        let kind = if text.starts_with("[browser test]") { "test" } else { "user" };
        s.events.push(json!({ "kind": kind, "text": text }));
        s.busy = true;
        s.last_used = self.clock.now();
        s.cancel.store(false, Ordering::Relaxed);
        let cancel = Arc::clone(&s.cancel);
        drop(s);

        let (fs, hub, checker, clock) = (Arc::clone(&self.fs), Arc::clone(&self.hub), Arc::clone(&self.checker), Arc::clone(&self.clock));
        let worker = Arc::clone(&session);
        self.tasks.spawn(Box::new(move || {
            let shop = Workshop { api: &api, fs: &*fs, hub: &hub, checker: &checker, clock: &*clock };
            let result = run_turn(&shop, &worker, &cancel);
            let mut s = worker.lock().unwrap();
            match result {
                Ok(()) => {}
                Err(e) if e == "stopped" => s.events.push(json!({ "kind": "stopped", "text": "Stopped. Nothing after the last save was kept." })),
                Err(e) => s.events.push(json!({ "kind": "error", "text": e })),
            }
            s.busy = false;
        }));
        Ok(json!({ "session": id }))
    }

    fn remember(&self, id: &str, s: Arc<Mutex<Session>>) {
        let mut all = self.sessions.lock().unwrap();
        if all.len() >= self.agent.lock().unwrap().limit("max_sessions") as usize {
            let oldest = all
                .iter()
                .filter(|(_, s)| s.try_lock().is_ok_and(|s| !s.busy))
                .min_by_key(|(_, s)| s.lock().unwrap().last_used)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                all.remove(&k);
            }
        }
        all.insert(id.to_string(), s);
    }

    /// The chat's events from index `since` on, so the page can poll cheaply.
    pub fn events(&self, id: &str, since: usize) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("that chat has ended (Wardian restarted)")?;
        let s = s.lock().unwrap();
        let from = since.min(s.events.len());
        Ok(json!({ "events": s.events[from..], "next": s.events.len(), "busy": s.busy, "app": s.app }))
    }

    /// The chats of the last day, newest first, with what each needs: so any tab can show
    /// "working", pick up a chat, or run the browser test of a save nobody has tested yet.
    pub fn sessions(&self) -> Value {
        let now = self.clock.now();
        let mut out: Vec<(u64, Value)> = self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, s)| {
                let s = s.lock().unwrap();
                (now.saturating_sub(s.last_used) < 24 * 3600).then(|| (s.last_used, s.summary(id, now)))
            })
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0));
        Value::Array(out.into_iter().map(|(_, v)| v).collect())
    }

    /// A tab asks to test save number `saved`. Yes only for the latest save, untested, and not
    /// being tested by another tab. Then every tab shows "Trying the app…".
    pub fn claim_test(&self, id: &str, saved: usize) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("that chat has ended (Wardian restarted)")?;
        let ok = s.lock().unwrap().claim_test(saved, self.clock.now());
        Ok(json!({ "claimed": ok }))
    }

    /// The result of a tab's browser test of save number `saved`.
    pub fn tested(&self, id: &str, body: &Value) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("that chat has ended (Wardian restarted)")?;
        let saved = usize::try_from(body["saved"].as_u64().ok_or("which save was tested?")?).map_err(|_| "which save was tested?")?;
        let recorded = s.lock().unwrap().record_test(saved, body["ok"].as_bool() == Some(true), body["text"].as_str().unwrap_or(""), self.clock.now());
        Ok(json!({ "recorded": recorded }))
    }

    pub fn stop(&self, id: &str) -> Result<Value, String> {
        let s = self.sessions.lock().unwrap().get(id).cloned().ok_or("no such chat")?;
        s.lock().unwrap().cancel.store(true, Ordering::Relaxed);
        Ok(json!({ "stopping": true }))
    }
}


fn push_event(session: &Mutex<Session>, ev: Value) {
    session.lock().unwrap().events.push(ev);
}

/// The package Claude works on, in memory: path -> bytes.
/// Reads an app folder, leaving out hidden files and build folders.
fn load(fs: &dyn FileSystem, dir: &Path) -> Files {
    fs.walk(dir)
        .into_iter()
        .filter(|(rel, _)| !rel.split('/').any(|p| p.starts_with('.') || SKIP_DIRS.contains(&p)))
        .filter_map(|(rel, _)| fs.read(&dir.join(&rel)).map(|b| (rel, b)))
        .collect()
}

/// Writes the package to `dir`, adding the empty app.wasm a page app needs.
fn materialize(fs: &dyn FileSystem, files: &Files, dir: &Path) -> Result<(), String> {
    for (rel, bytes) in files {
        fs.write(&dir.join(rel), bytes)?;
    }
    if !files.contains_key("suite.json") && !files.contains_key("app.wasm") && files.contains_key("app.json") {
        fs.write(&dir.join("app.wasm"), EMPTY_WASM)?;
    }
    Ok(())
}

/// A hidden folder inside the apps folder, so the final move is one rename.
fn staging_dir(root: &Path, clock: &dyn Clock) -> PathBuf {
    root.join(format!(".ai-{}-{:08x}", clock.now(), clock.nonce()))
}

/// Checks the package, then saves it as the local app `name` in place. The version it replaces is
/// already in the app's history (ADR-2610071122); the new one is recorded as made `by`. Returns the
/// version it replaced (if any) and the new one. Make an app and App settings both save this way.
fn save_package(fs: &dyn FileSystem, hub: &Hub, checker: &Checker, clock: &dyn Clock, files: &Files, name: &str, by: &str, why: &str) -> Result<(Option<u64>, Option<u64>), String> {
    let root = hub.local_root();
    let dir = staging_dir(root, clock);
    fs.create_dir_all(root)?;
    if let Err(e) = materialize(fs, files, &dir) {
        fs.remove_dir_all(&dir);
        return Err(format!("cannot write the files: {e}"));
    }
    let (ok, report) = checker.check_dir(&dir, name);
    if !ok {
        fs.remove_dir_all(&dir);
        return Err(format!("not saved: the check found errors.\n{report}"));
    }
    let history = hub.history();
    let target = root.join(name);
    let replaced = if fs.exists(&target) { history.before_change(name) } else { None };
    // Move the current copy aside first, so the app is never missing for longer than a rename.
    let aside = root.join(format!(".old-{name}-{:08x}", clock.nonce()));
    if fs.exists(&target) {
        if let Err(e) = fs.rename(&target, &aside) {
            fs.remove_dir_all(&dir);
            return Err(format!("cannot replace {name}: {e}"));
        }
    }
    if let Err(e) = fs.rename(&dir, &target) {
        let _ = fs.rename(&aside, &target);
        fs.remove_dir_all(&dir);
        return Err(format!("cannot save {name}: {e}"));
    }
    fs.remove_dir_all(&aside);
    let version = history.record(name, by, why);
    println!("{by}: saved {name}");
    Ok((replaced, version))
}

impl Workshop<'_> {
    /// Checks, then saves in place (`save_package`). Returns the app name, the version it replaced
    /// (if any) and the new one.
    fn save(&self, files: &Files, app: Option<&str>, wanted: &str, why: &str) -> Result<(String, Option<u64>, Option<u64>), String> {
        let name = match app {
            Some(a) => a.to_string(),
            None => free_name(wanted, &|n| self.fs.exists(&self.hub.local_root().join(n))),
        };
        let (replaced, version) = save_package(self.fs, self.hub, self.checker, self.clock, files, &name, "make-an-app", why)?;
        Ok((name, replaced, version))
    }

    /// Runs one tool. Returns the text for Claude, whether it is an error, and
    /// the line to show the user (if any).
    fn run_tool(&self, name: &str, input: &Value, files: &mut Files, session: &Mutex<Session>, cancel: &AtomicBool) -> (String, bool, Option<String>) {
        match name {
            workshop::SKILLS_TOOL => {
                push_event(session, json!({ "kind": "tool", "text": "asked the skills agent how to give the app's AI its skill and tools" }));
                match self.api.skills(input, files, cancel) {
                    Ok(out) => (out.text, out.is_error, out.shown),
                    Err(e) => (e, true, None),
                }
            }
            "finish" => {
                let app = session.lock().unwrap().app.clone();
                let wanted = input["name"].as_str().unwrap_or("my-app");
                let summary = input["summary"].as_str().unwrap_or("").to_string();
                match self.save(files, app.as_deref(), wanted, &summary) {
                    Ok((name, replaced, version)) => {
                        let mut s = session.lock().unwrap();
                        s.app = Some(name.clone());
                        s.events.push(json!({ "kind": "saved", "app": name, "replaced": replaced, "version": version, "text": summary }));
                        (format!("Saved as \"{name}\". The user can open it now."), false, None)
                    }
                    Err(e) => (e, true, Some("not saved yet: the check found problems; fixing them".into())),
                }
            }
            // Every other tool works on the files in memory, as the hosted builder runs them.
            _ => {
                let app = session.lock().unwrap().app.clone();
                let out = workshop::run(&*self.api.assets, name, input, files, app.as_deref());
                (out.text, out.is_error, out.shown)
            }
        }
    }
}

/// One turn: Claude works until it stops calling tools, with a limit on steps.
fn run_turn(shop: &Workshop, session: &Mutex<Session>, cancel: &AtomicBool) -> Result<(), String> {
    let (mut messages, app) = {
        let s = session.lock().unwrap();
        (s.messages.clone(), s.app.clone())
    };
    // Start from the app as it is on disk now, so edits made by hand are kept.
    let mut files = Files::new();
    if let Some(app) = &app {
        files = load(shop.fs, &shop.hub.local_root().join(app));
        files.retain(|p, b| !(p == "app.wasm" && b.as_slice() == EMPTY_WASM));
    }
    let start_len = messages.len();
    let mut saved_since_change = true;
    let mut nudged = false;
    let result = (|| {
        for _ in 0..shop.api.max_steps {
            if cancel.load(Ordering::Relaxed) {
                return Err("stopped".to_string());
            }
            let reply = shop.api.messages(&messages, cancel)?;
            // An empty text block is refused when sent back, so drop any.
            let content: Vec<Value> = reply["content"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|b| !(b["type"] == "text" && b["text"].as_str().is_none_or(|t| t.trim().is_empty())))
                .collect();
            if content.is_empty() {
                return Ok(());
            }
            for block in &content {
                if block["type"] == "text" {
                    let t = block["text"].as_str().unwrap_or("").trim();
                    if !t.is_empty() {
                        push_event(session, json!({ "kind": "say", "text": t }));
                    }
                }
            }
            messages.push(json!({ "role": "assistant", "content": content }));
            let calls: Vec<&Value> = content.iter().filter(|b| b["type"] == "tool_use").collect();
            let cut_off = reply["stop_reason"] == "max_tokens";
            if calls.is_empty() {
                if cut_off {
                    messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": "Your reply was cut off at the length limit. Continue, and write large files in parts with append." }] }));
                    continue;
                }
                if !saved_since_change && !nudged {
                    nudged = true;
                    messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": "You changed files but did not call finish, so nothing is saved. Call finish now, or say what stops you." }] }));
                    continue;
                }
                return Ok(());
            }
            let mut results = Vec::new();
            for call in calls {
                let name = call["name"].as_str().unwrap_or("");
                let (text, is_error, shown) = if cut_off && name != "finish" {
                    ("Your reply was cut off at the length limit, so this call was not run. Write large files in smaller parts with append.".to_string(), true, None)
                } else {
                    shop.run_tool(name, &call["input"], &mut files, session, cancel)
                };
                match name {
                    "write_file" | "delete_file" if !is_error => saved_since_change = false,
                    "finish" if !is_error => saved_since_change = true,
                    _ => {}
                }
                if let Some(line) = shown {
                    push_event(session, json!({ "kind": "tool", "text": line }));
                }
                results.push(json!({ "type": "tool_result", "tool_use_id": call["id"], "content": text, "is_error": is_error }));
            }
            messages.push(json!({ "role": "user", "content": results }));
        }
        Err(format!("Claude used all {} steps for this message. Send another message to continue.", shop.api.max_steps))
    })();
    // Keep the conversation only up to a complete exchange, so the next
    // message always follows a valid history.
    let mut s = session.lock().unwrap();
    if result.is_ok() || messages.last().is_some_and(|m| m["role"] == "user" && m["content"][0]["type"] == "tool_result") {
        s.messages = messages;
    } else if messages.len() > start_len {
        // Stopped or failed mid-request: drop the unanswered last step, keep the rest.
        while messages.len() > start_len && messages.last().is_some_and(|m| m["role"] == "assistant") {
            messages.pop();
        }
        s.messages = messages;
    }
    result
}

impl Builder for Studio {
    fn status(&self) -> Value {
        Studio::status(self)
    }
    fn set_key(&self, key: &str, workspace: Option<&str>) -> Result<Value, String> {
        Studio::set_key(self, key, workspace)
    }
    fn set_provider(&self, body: &Value) -> Result<Value, String> {
        Studio::set_provider(self, body)
    }
    fn aws_profiles(&self) -> Value {
        Studio::aws_profiles(self)
    }
    fn send(&self, body: &Value) -> Result<Value, String> {
        Studio::send(self, body)
    }
    fn sample(&self, body: &Value) -> Result<Value, String> {
        Studio::sample(self, body)
    }
    fn stop(&self, session: &str) -> Result<Value, String> {
        Studio::stop(self, session)
    }
    fn claim_test(&self, session: &str, saved: usize) -> Result<Value, String> {
        Studio::claim_test(self, session, saved)
    }
    fn tested(&self, session: &str, body: &Value) -> Result<Value, String> {
        Studio::tested(self, session, body)
    }
    fn sessions(&self) -> Value {
        Studio::sessions(self)
    }
    fn events(&self, session: &str, since: usize) -> Result<Value, String> {
        Studio::events(self, session, since)
    }
    fn agent(&self) -> Value {
        Studio::agent(self)
    }
    fn set_agent(&self, patch: &Value) -> Result<Value, String> {
        Studio::set_agent(self, patch)
    }
    fn usage(&self) -> Value {
        Studio::usage(self)
    }
    fn ai_tools(&self, app: &str) -> Result<Value, String> {
        Studio::ai_tools(self, app)
    }
    fn save_ai_tools(&self, app: &str, body: &Value) -> Result<Value, String> {
        Studio::save_ai_tools(self, app, body)
    }
    fn improve_ai_tools(&self, app: &str, body: &Value) -> Result<Value, String> {
        Studio::improve_ai_tools(self, app, body)
    }
}

impl KeyOwner for Studio {
    fn entries(&self) -> Vec<KeyEntry> {
        let unreadable = self.unreadable.lock().unwrap();
        let key = self.key.lock().unwrap();
        let ws = self.workspace.lock().unwrap();
        let anthropic = KeyEntry {
            id: "anthropic",
            name: "Anthropic API key",
            from: key.as_ref().map(|(_, from)| *from),
            detail: match &*key {
                Some((k, _)) if ws.is_empty() => format!("{}, model {}", key_hint(k), self.model_of(Provider::Anthropic, Tier::Main)),
                Some((k, _)) => format!("{}, workspace {ws}, model {}", key_hint(k), self.model_of(Provider::Anthropic, Tier::Main)),
                None => "not set".into(),
            },
            error: unreadable.get("anthropic").cloned(),
        };
        let bedrock = self.bedrock.lock().unwrap();
        let bedrock = KeyEntry {
            id: "bedrock",
            name: "Amazon Bedrock",
            from: bedrock.as_ref().map(|(_, from)| *from),
            detail: match &*bedrock {
                Some((b, _)) => format!("region {}, {}, model {}", b.region, b.auth_words(), self.model_of(Provider::Bedrock, Tier::Main)),
                None => "not set".into(),
            },
            error: unreadable.get("bedrock").cloned(),
        };
        vec![anthropic, bedrock]
    }

    fn retest(&self, id: &str) -> Option<Result<(), String>> {
        match id {
            "anthropic" => Some((|| {
                let key = self.key.lock().unwrap().as_ref().map(|(k, _)| k.clone()).ok_or("no Anthropic API key is set")?;
                let ws = self.workspace.lock().unwrap().clone();
                self.test_anthropic(&key, &ws)
            })()),
            "bedrock" => Some((|| {
                let b = self.bedrock.lock().unwrap().as_ref().map(|(b, _)| b.clone()).ok_or("Amazon Bedrock is not set up")?;
                self.test_bedrock(&b)
            })()),
            _ => None,
        }
    }

    fn forget(&self, id: &str) -> Option<Result<(), String>> {
        match id {
            "anthropic" => {
                self.forget_anthropic();
                Some(Ok(()))
            }
            "bedrock" => {
                self.forget_bedrock();
                Some(Ok(()))
            }
            _ => None,
        }
    }
}

/// The last four characters of a key, which is how the consoles name keys too. A short key is
/// not named at all.
fn key_hint(k: &str) -> String {
    let chars: Vec<char> = k.chars().collect();
    if chars.len() < 20 {
        return "a key".into();
    }
    format!("key ending {}", chars[chars.len() - 4..].iter().collect::<String>())
}
