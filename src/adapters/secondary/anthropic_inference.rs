//! The Claude port over the Anthropic Messages API. This is the one place that names models:
//! WARDIAN_AI_MODEL (or RUSTLE_AI_MODEL) chooses the main one, and Settings can choose others
//! (ADR-2610081500), which the use case then names in each request.

use crate::ports::llm::{Llm, LlmAuth, LlmError, Tier};
use serde_json::Value;
use std::time::Duration;

const DEFAULT_MODEL: &str = "claude-opus-5-5";
const QUICK_MODEL: &str = "claude-haiku-4-5-20251001";
pub const DEFAULT_BASE: &str = "https://api.anthropic.com";

pub struct Anthropic {
    base: String,
    model: String,
}

impl Anthropic {
    /// `base` is the API address (ANTHROPIC_BASE_URL, for tests); `model` the main model, if chosen.
    pub fn new(base: &str, model: Option<String>) -> Anthropic {
        Anthropic { base: base.trim_end_matches('/').to_string(), model: model.unwrap_or_else(|| DEFAULT_MODEL.into()) }
    }

    fn request(&self, auth: &LlmAuth, method: &str, path: &str) -> Result<ureq::Request, LlmError> {
        let LlmAuth::Anthropic { key, workspace } = auth else {
            return Err(LlmError::Transport("these are not Anthropic API credentials".into()));
        };
        let req = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(600))
            .build()
            .request(method, &format!("{}{path}", self.base))
            .set("x-api-key", key)
            .set("anthropic-version", "2023-06-01");
        Ok(if workspace.is_empty() { req } else { req.set("anthropic-workspace-id", workspace) })
    }
}

fn failure(e: ureq::Error) -> LlmError {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: Value = resp.into_json().unwrap_or_default();
            LlmError::Status(code, body["error"]["message"].as_str().unwrap_or("").to_string())
        }
        ureq::Error::Transport(t) => LlmError::Transport(t.message().map(String::from).unwrap_or_else(|| t.kind().to_string())),
    }
}

impl Llm for Anthropic {
    fn model(&self, tier: Tier) -> String {
        match tier {
            Tier::Main => self.model.clone(),
            Tier::Quick => QUICK_MODEL.into(),
        }
    }

    fn messages(&self, auth: &LlmAuth, body: &Value) -> Result<Value, LlmError> {
        let resp = self.request(auth, "POST", "/v1/messages")?.send_json(body.clone()).map_err(failure)?;
        resp.into_json().map_err(|e| LlmError::Unreadable(e.to_string()))
    }

    /// Looks the model up: a refused key answers 401, and a model the key cannot use 404.
    fn test_key(&self, auth: &LlmAuth, model: &str) -> Result<(), LlmError> {
        self.request(auth, "GET", &format!("/v1/models/{model}"))?.call().map(drop).map_err(failure)
    }
}
