//! Claude's settings in Wardian (ADR-2610081500): the model for each provider and tier, the limits
//! of Make an app and of `claude:sample`, and the daily token caps. Kept in `agent.json`.

use super::package::safe_segment;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// A number setting: its name, default, and the smallest and largest value allowed.
pub struct Limit {
    pub name: &'static str,
    pub default: u64,
    pub min: u64,
    pub max: u64,
}

/// Every number setting. A cap of 0 means no cap.
pub const LIMITS: [Limit; 6] = [
    Limit { name: "max_steps", default: 40, min: 5, max: 200 },
    Limit { name: "max_tokens", default: 16_000, min: 1_000, max: 64_000 },
    Limit { name: "max_sessions", default: 20, min: 1, max: 100 },
    Limit { name: "sample_max_tokens", default: 4_000, min: 256, max: 16_000 },
    Limit { name: "sample_daily_tokens", default: 200_000, min: 0, max: 100_000_000 },
    Limit { name: "build_daily_tokens", default: 0, min: 0, max: 100_000_000 },
];

const PROVIDERS: [&str; 2] = ["anthropic", "bedrock"];

/// The payer of Make an app's requests in usage. The space makes it a name no package can have.
pub const BUILDER: &str = "Make an app";

/// The models chosen for one provider; an empty one means the adapter's default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Models {
    pub main: String,
    pub quick: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentSettings {
    /// By provider: "anthropic" or "bedrock".
    pub models: BTreeMap<String, Models>,
    /// By the names in `LIMITS`.
    limits: BTreeMap<&'static str, u64>,
    /// A daily cap for one package, in place of `sample_daily_tokens`.
    pub caps: BTreeMap<String, u64>,
}

impl Default for AgentSettings {
    fn default() -> AgentSettings {
        AgentSettings {
            models: PROVIDERS.iter().map(|p| (p.to_string(), Models::default())).collect(),
            limits: LIMITS.iter().map(|l| (l.name, l.default)).collect(),
            caps: BTreeMap::new(),
        }
    }
}

/// 1 to 200 of `A-Z a-z 0-9 . _ : - /`: every Anthropic model name, Bedrock model id, inference
/// profile and ARN, and nothing that could change a URL's meaning.
fn valid_model(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-' | '/'))
}

impl AgentSettings {
    pub fn limit(&self, name: &str) -> u64 {
        self.limits.get(name).copied().unwrap_or(0)
    }

    /// The model chosen for `provider`, or None for the adapter's default.
    pub fn model(&self, provider: &str, quick: bool) -> Option<&str> {
        let m = self.models.get(provider)?;
        Some(if quick { &m.quick } else { &m.main }).map(String::as_str).filter(|s| !s.is_empty())
    }

    /// Today's cap for a package's `claude:sample`, or None for no cap.
    pub fn sample_cap(&self, package: &str) -> Option<u64> {
        Some(self.caps.get(package).copied().unwrap_or(self.limit("sample_daily_tokens"))).filter(|&c| c > 0)
    }

    /// Make an app's daily cap, or None for no cap.
    pub fn build_cap(&self) -> Option<u64> {
        Some(self.limit("build_daily_tokens")).filter(|&c| c > 0)
    }

    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        out.insert(
            "models".into(),
            self.models.iter().map(|(p, m)| (p.clone(), json!({ "main": m.main, "quick": m.quick }))).collect::<Map<_, _>>().into(),
        );
        for (k, v) in &self.limits {
            out.insert(k.to_string(), json!(v));
        }
        out.insert("caps".into(), json!(self.caps));
        Value::Object(out)
    }

    /// The settings with `patch` applied: a field it names replaces this one, a field it leaves
    /// out stays, and a cap set to null is removed. Every value is checked; one bad value refuses
    /// the whole patch.
    pub fn apply(&self, patch: &Value) -> Result<AgentSettings, String> {
        let patch = patch.as_object().ok_or("the settings must be a JSON object")?;
        let mut next = self.clone();
        for (key, value) in patch {
            match key.as_str() {
                "models" => {
                    let by_provider = value.as_object().ok_or("models must be an object by provider")?;
                    for (provider, m) in by_provider {
                        let slot = next.models.get_mut(provider).ok_or_else(|| format!("unknown provider \"{provider}\": use anthropic or bedrock"))?;
                        for (tier, name) in m.as_object().ok_or("each provider's models must be an object")? {
                            let name = name.as_str().ok_or("a model must be a string")?.trim();
                            if !name.is_empty() && !valid_model(name) {
                                return Err(format!("\"{name}\" is not a model name (1 to 200 letters, digits and . _ : - /)"));
                            }
                            match tier.as_str() {
                                "main" => slot.main = name.into(),
                                "quick" => slot.quick = name.into(),
                                other => return Err(format!("unknown tier \"{other}\": use main or quick")),
                            }
                        }
                    }
                }
                "caps" => {
                    for (package, cap) in value.as_object().ok_or("caps must be an object by app")? {
                        if !safe_segment(package) {
                            return Err(format!("\"{package}\" is not an app name"));
                        }
                        if cap.is_null() {
                            next.caps.remove(package);
                        } else {
                            next.caps.insert(package.clone(), in_range("a cap", cap, 0, 100_000_000)?);
                        }
                    }
                }
                name => {
                    let limit = LIMITS.iter().find(|l| l.name == name).ok_or_else(|| format!("unknown setting \"{name}\""))?;
                    next.limits.insert(limit.name, in_range(limit.name, value, limit.min, limit.max)?);
                }
            }
        }
        Ok(next)
    }

    /// `agent.json` as saved. A file that does not pass `apply` is reported, and the defaults apply.
    pub fn from_saved(v: &Value) -> Result<AgentSettings, String> {
        AgentSettings::default().apply(v)
    }
}

fn in_range(name: &str, v: &Value, min: u64, max: u64) -> Result<u64, String> {
    v.as_u64().filter(|n| (min..=max).contains(n)).ok_or_else(|| format!("{name} must be a whole number from {min} to {max}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_defaults_are_the_old_constants() {
        let s = AgentSettings::default();
        assert_eq!((s.limit("max_steps"), s.limit("max_tokens"), s.limit("max_sessions")), (40, 16_000, 20));
        assert_eq!(s.limit("sample_max_tokens"), 4_000);
        assert_eq!(s.sample_cap("any"), Some(200_000));
        assert_eq!(s.build_cap(), None);
        assert_eq!(s.model("anthropic", false), None);
    }

    #[test]
    fn agent_limits_outside_their_bounds_are_refused() {
        let s = AgentSettings::default();
        for (name, bad) in [("max_steps", 4), ("max_steps", 201), ("max_tokens", 999), ("max_sessions", 0), ("sample_max_tokens", 16_001), ("build_daily_tokens", 100_000_001)] {
            assert!(s.apply(&json!({ name: bad })).is_err(), "{name} {bad}");
        }
        assert!(s.apply(&json!({ "max_steps": "40" })).is_err(), "a string is not a number");
        assert!(s.apply(&json!({ "max_steps": -1 })).is_err());
        assert!(s.apply(&json!({ "nonsense": 1 })).is_err());
        let ok = s.apply(&json!({ "max_steps": 200, "max_tokens": 1_000 })).unwrap();
        assert_eq!((ok.limit("max_steps"), ok.limit("max_tokens")), (200, 1_000));
    }

    #[test]
    fn agent_one_bad_value_refuses_the_whole_patch() {
        let s = AgentSettings::default();
        assert!(s.apply(&json!({ "max_steps": 50, "max_tokens": 5 })).is_err());
        assert_eq!(s.limit("max_steps"), 40);
    }

    #[test]
    fn agent_model_names_are_checked() {
        let s = AgentSettings::default();
        for good in ["claude-opus-5-5", "us.anthropic.claude-sonnet-4-5-20250929-v1:0", "arn:aws:bedrock:us-east-1:123:inference-profile/x"] {
            assert!(s.apply(&json!({ "models": { "bedrock": { "main": good } } })).is_ok(), "{good}");
        }
        for bad in ["a b", "x?y", "../../v1/x#", &"m".repeat(201)] {
            assert!(s.apply(&json!({ "models": { "anthropic": { "main": bad } } })).is_err(), "{bad}");
        }
        assert!(s.apply(&json!({ "models": { "openai": { "main": "gpt" } } })).is_err());
        assert!(s.apply(&json!({ "models": { "anthropic": { "large": "x" } } })).is_err());
        let set = s.apply(&json!({ "models": { "anthropic": { "quick": "claude-haiku-5-5" } } })).unwrap();
        assert_eq!(set.model("anthropic", true), Some("claude-haiku-5-5"));
        assert_eq!(set.model("anthropic", false), None, "an empty main model stays the default");
        let cleared = set.apply(&json!({ "models": { "anthropic": { "quick": "" } } })).unwrap();
        assert_eq!(cleared.model("anthropic", true), None);
    }

    #[test]
    fn agent_caps_per_app_replace_the_default_and_null_removes_one() {
        let s = AgentSettings::default().apply(&json!({ "caps": { "notes": 5_000, "free": 0 } })).unwrap();
        assert_eq!(s.sample_cap("notes"), Some(5_000));
        assert_eq!(s.sample_cap("free"), None, "0 is no cap");
        assert_eq!(s.sample_cap("other"), Some(200_000));
        let s = s.apply(&json!({ "caps": { "notes": null } })).unwrap();
        assert_eq!(s.sample_cap("notes"), Some(200_000));
        assert!(s.apply(&json!({ "caps": { "../x": 1 } })).is_err());
        assert!(s.apply(&json!({ "sample_daily_tokens": 0 })).unwrap().sample_cap("x").is_none());
    }

    #[test]
    fn agent_settings_survive_a_round_trip() {
        let s = AgentSettings::default()
            .apply(&json!({ "max_steps": 60, "caps": { "a": 9 }, "models": { "bedrock": { "main": "m1", "quick": "m2" } } }))
            .unwrap();
        assert_eq!(AgentSettings::from_saved(&s.to_json()).unwrap(), s);
    }
}
