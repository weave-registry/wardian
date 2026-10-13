//! An AI app's skill and tools as data (ADR-2610131300): the repository the out-of-sight `ai` app
//! loads, which App settings shows, edits and improves with Claude. The skill is `skill.md`; the
//! tools are `tools.json`, each one's name, description and input schema, whether it is on, and
//! whether it is only proposed (written down, with no code in the ai app yet). A tool's `run` is
//! the ai app's own JavaScript and is never part of the repository.

use serde_json::{json, Value};
use std::collections::HashSet;

/// The repository's files, in the ai app's folder.
pub const SKILL_FILE: &str = "skill.md";
pub const TOOLS_FILE: &str = "tools.json";
/// The newest tools.json format this Wardian reads.
pub const FORMAT: u64 = 1;

/// Limits that keep the skill, the tools and a task inside one claude:sample prompt (60,000
/// characters; agent.js keeps 58,000 for the whole prompt).
pub const MAX_SKILL_CHARS: usize = 20_000;
pub const MAX_TOOLS: usize = 32;
pub const MAX_DESCRIPTION_CHARS: usize = 2_000;
const MAX_SCHEMA_CHARS: usize = 4_000;
pub const MAX_TOTAL_CHARS: usize = 40_000;

/// Tools agent.js runs itself, on the package's own database, for any ai app with the `db`
/// capability: listed in tools.json like the others, with no code in the app.
pub const BUILTIN_TOOLS: &[&str] = &["list_tables", "read_rows"];

/// The built-in tools as App settings adds them. agent.js holds the same input schemas and always
/// checks against its own, so an edited schema cannot widen what they read.
pub fn builtin_tools() -> Value {
    json!([
        { "name": "list_tables", "description": "The tables in this app's database, with their columns. Use it first, to know what there is to read.",
          "input": { "type": "object", "properties": {}, "additionalProperties": false }, "enabled": true, "proposed": false },
        { "name": "read_rows", "description": "Up to 50 rows of one table, from offset, optionally ordered by a column. Use it to look at the data itself; page through with offset rather than asking for everything.",
          "input": { "type": "object", "properties": {
              "table": { "type": "string", "minLength": 1, "maxLength": 64 },
              "offset": { "type": "integer", "minimum": 0 },
              "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
              "orderBy": { "type": "string", "maxLength": 64 },
              "desc": { "type": "boolean" } }, "required": ["table"], "additionalProperties": false },
          "enabled": true, "proposed": false }
    ])
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input: Value,
    pub enabled: bool,
    pub proposed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repository {
    pub skill: String,
    pub tools: Vec<Tool>,
}

/// A snake_case name, as agent.js requires.
fn valid_name(n: &str) -> bool {
    let b = n.as_bytes();
    !b.is_empty() && b.len() <= 64 && b[0].is_ascii_lowercase() && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

impl Repository {
    /// From the editor or Claude: {skill, tools: [{name, description, input, enabled, proposed}]}.
    /// Every problem is named, so the editor can show them all at once.
    pub fn from_json(v: &Value) -> Result<Repository, String> {
        let mut problems = Vec::new();
        let skill = match &v["skill"] {
            Value::Null => String::new(),
            Value::String(s) => s.replace("\r\n", "\n"),
            _ => return Err("skill must be text".into()),
        };
        if skill.chars().count() > MAX_SKILL_CHARS {
            problems.push(format!("the skill is longer than {MAX_SKILL_CHARS} characters"));
        }
        let list = match &v["tools"] {
            Value::Null => &[][..],
            Value::Array(a) => &a[..],
            _ => return Err("tools must be a list".into()),
        };
        if list.len() > MAX_TOOLS {
            problems.push(format!("at most {MAX_TOOLS} tools"));
        }
        let mut seen = HashSet::new();
        let mut tools = Vec::new();
        for (i, t) in list.iter().enumerate() {
            let name = t["name"].as_str().unwrap_or("").trim().to_string();
            let at = if name.is_empty() { format!("tool {}", i + 1) } else { name.clone() };
            if !t.is_object() {
                problems.push(format!("{at}: must be an object"));
                continue;
            }
            if !valid_name(&name) {
                problems.push(format!("{at}: the name must be snake_case (a-z, 0-9, _), starting with a letter"));
            } else if !seen.insert(name.clone()) {
                problems.push(format!("{at}: two tools have this name"));
            }
            let description = t["description"].as_str().unwrap_or("").trim().to_string();
            if description.is_empty() {
                problems.push(format!("{at}: say when to use it and what it returns"));
            } else if description.chars().count() > MAX_DESCRIPTION_CHARS {
                problems.push(format!("{at}: the description is longer than {MAX_DESCRIPTION_CHARS} characters"));
            }
            let input = match &t["input"] {
                Value::Null => json!({ "type": "object", "properties": {} }),
                i => i.clone(),
            };
            if let Err(e) = check_schema(&input) {
                problems.push(format!("{at}: input {e}"));
            } else if input.to_string().len() > MAX_SCHEMA_CHARS {
                problems.push(format!("{at}: the input schema is longer than {MAX_SCHEMA_CHARS} characters"));
            }
            let flag = |k: &str, default: bool| t[k].as_bool().unwrap_or(default);
            tools.push(Tool { name, description, input, enabled: flag("enabled", true), proposed: flag("proposed", false) });
        }
        let repo = Repository { skill, tools };
        if problems.is_empty() && repo.size() > MAX_TOTAL_CHARS {
            problems.push(format!("the skill and tools come to {} characters; keep them under {MAX_TOTAL_CHARS}, so a question still fits", repo.size()));
        }
        if problems.is_empty() { Ok(repo) } else { Err(problems.join("\n")) }
    }

    /// From the ai app's files; either may be missing.
    pub fn from_files(skill: Option<&[u8]>, tools: Option<&[u8]>) -> Result<Repository, String> {
        let skill = skill.map(|b| String::from_utf8_lossy(b).trim_end().to_string()).unwrap_or_default();
        let tools = match tools {
            None => json!([]),
            Some(b) => {
                let v: Value = serde_json::from_slice(b).map_err(|e| format!("{TOOLS_FILE} is not valid JSON: {e}"))?;
                if v["format"].as_u64().unwrap_or(1) > FORMAT {
                    return Err(format!("{TOOLS_FILE} has format {}; this Wardian reads format {FORMAT}", v["format"]));
                }
                v["tools"].clone()
            }
        };
        Repository::from_json(&json!({ "skill": skill, "tools": tools })).map_err(|e| format!("{TOOLS_FILE}: {e}"))
    }

    pub fn to_json(&self) -> Value {
        json!({ "skill": self.skill, "tools": self.tools.iter().map(Tool::to_json).collect::<Vec<_>>() })
    }

    /// skill.md and tools.json, as saved.
    pub fn to_files(&self) -> (Vec<u8>, Vec<u8>) {
        let mut skill = self.skill.trim_end().to_string();
        skill.push('\n');
        let tools = json!({ "format": FORMAT, "tools": self.tools.iter().map(Tool::to_json).collect::<Vec<_>>() });
        let tools = serde_json::to_string_pretty(&tools).unwrap_or_default() + "\n";
        (skill.into_bytes(), tools.into_bytes())
    }

    /// The characters the model sees of it.
    pub fn size(&self) -> usize {
        self.skill.chars().count() + self.tools.iter().map(|t| t.name.len() + t.description.chars().count() + t.input.to_string().len()).sum::<usize>()
    }
}

impl Tool {
    fn to_json(&self) -> Value {
        json!({ "name": self.name, "description": self.description, "input": self.input, "enabled": self.enabled, "proposed": self.proposed })
    }
}

/// An input schema agent.js can check: an object schema, using only the keywords it knows.
fn check_schema(s: &Value) -> Result<(), String> {
    let Value::Object(o) = s else { return Err("must be a JSON schema object".into()) };
    if o.get("type").and_then(Value::as_str) != Some("object") {
        return Err("must have \"type\": \"object\"".into());
    }
    check_keywords(s, "input", 0)
}

const KEYWORDS: &[&str] = &[
    "type", "description", "enum", "required", "properties", "items", "minimum", "maximum", "minLength", "maxLength", "maxItems", "additionalProperties",
    "default", "title", "examples", "format",
];

fn check_keywords(s: &Value, at: &str, depth: usize) -> Result<(), String> {
    if depth > 8 {
        return Err("is nested too deeply".into());
    }
    let Value::Object(o) = s else { return Err(format!("{at} must be a schema object")) };
    for (k, v) in o {
        if !KEYWORDS.contains(&k.as_str()) {
            return Err(format!("uses \"{k}\", which the agent does not check; use type, enum, required, properties, items, minimum, maximum, minLength, maxLength, maxItems or additionalProperties"));
        }
        match k.as_str() {
            "properties" => {
                let Value::Object(p) = v else { return Err(format!("{at}.properties must be an object")) };
                for (name, sub) in p {
                    check_keywords(sub, &format!("{at}.{name}"), depth + 1)?;
                }
            }
            "items" => check_keywords(v, &format!("{at}[]"), depth + 1)?,
            "required" if !v.as_array().is_some_and(|a| a.iter().all(Value::is_string)) => return Err(format!("{at}.required must be a list of names")),
            "additionalProperties" if !v.is_boolean() => return Err(format!("{at}.additionalProperties must be true or false")),
            _ => {}
        }
    }
    Ok(())
}

/// The ai app of a suite.json, if it has one: its folder, and whether it can explore the
/// package's database and answer the chat.
pub struct AiApp {
    pub dir: String,
    pub db: bool,
    pub chat: bool,
}

pub fn ai_app(suite: &Value, name: &str) -> Option<AiApp> {
    let a = suite["apps"].as_array()?.iter().find(|a| a["name"] == name)?;
    let has = |k: &str, v: &str| a[k].as_array().is_some_and(|l| l.iter().any(|x| x == v));
    has("caps", "claude:sample").then(|| AiApp {
        dir: a["dir"].as_str().map(String::from).unwrap_or_else(|| format!("apps/{name}")),
        db: has("caps", "db"),
        chat: has("provides", "chat"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> Value {
        json!({ "name": name, "description": "The hosts with the most errors.", "input": { "type": "object", "properties": { "limit": { "type": "integer", "minimum": 1 } }, "required": ["limit"] } })
    }

    #[test]
    fn a_repository_round_trips_through_its_files() {
        let r = Repository::from_json(&json!({ "skill": "Answer from the data.", "tools": [tool("top_hosts"), { "name": "find_x", "description": "Finds x.", "proposed": true }] })).unwrap();
        assert!(r.tools[0].enabled && !r.tools[0].proposed && r.tools[1].proposed);
        assert_eq!(r.tools[1].input["type"], "object", "a missing schema is an empty object");
        let (skill, tools) = r.to_files();
        assert_eq!(Repository::from_files(Some(&skill), Some(&tools)).unwrap(), r);
        assert_eq!(Repository::from_files(None, None).unwrap(), Repository::default());
    }

    #[test]
    fn every_problem_is_named() {
        let e = Repository::from_json(&json!({ "tools": [tool("Bad-Name"), tool("ok"), tool("ok"), { "name": "no_words", "input": { "type": "string" } }] })).unwrap_err();
        for want in ["Bad-Name: the name must be snake_case", "ok: two tools have this name", "no_words: say when to use it", "no_words: input must have \"type\": \"object\""] {
            assert!(e.contains(want), "{want} in {e}");
        }
    }

    #[test]
    fn a_schema_uses_only_what_the_agent_checks() {
        let mut t = tool("x");
        t["input"]["properties"]["limit"]["pattern"] = json!("^a");
        assert!(Repository::from_json(&json!({ "tools": [t] })).unwrap_err().contains("\"pattern\""));
    }

    #[test]
    fn the_skill_and_tools_must_leave_room_for_a_question() {
        let long = "x".repeat(MAX_SKILL_CHARS + 1);
        assert!(Repository::from_json(&json!({ "skill": long })).unwrap_err().contains("longer than"));
        let tools: Vec<Value> = (0..30).map(|i| json!({ "name": format!("t{i}"), "description": "d".repeat(1900) })).collect();
        assert!(Repository::from_json(&json!({ "skill": "s".repeat(MAX_SKILL_CHARS), "tools": tools })).unwrap_err().contains("so a question still fits"));
    }

    #[test]
    fn a_newer_format_is_refused() {
        assert!(Repository::from_files(None, Some(br#"{"format": 2, "tools": []}"#)).unwrap_err().contains("format 2"));
    }

    #[test]
    fn the_ai_app_is_the_one_with_claude() {
        let s = json!({ "apps": [{ "name": "ai", "caps": ["claude:sample", "db"], "provides": ["ask", "chat"] }, { "name": "view", "slot": "main" }] });
        let a = ai_app(&s, "ai").unwrap();
        assert_eq!((a.dir.as_str(), a.db, a.chat), ("apps/ai", true, true));
        assert!(ai_app(&s, "view").is_none());
        assert!(ai_app(&json!({ "apps": [{ "name": "ai", "dir": "brain" }] }), "ai").is_none(), "no claude:sample, no AI");
    }
}
