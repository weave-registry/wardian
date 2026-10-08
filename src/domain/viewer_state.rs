//! State that belongs to whoever views Wardian, kept by the host (ADR-2610071055): each app's
//! Arrange layout, each suite app's saved data (`ctx.store`), and the latest message on each
//! channel. This module holds the rules for a valid change; the files are the use case's business.

use super::grants::valid_channel;
use super::package::safe_segment;
use serde_json::{Map, Value};

/// The largest layout, as JSON.
const MAX_LAYOUT_BYTES: usize = 20 * 1024;
/// The most one app of a suite may keep, as JSON.
const MAX_APP_BYTES: usize = 1024 * 1024;
/// The most one package may keep across its apps, as JSON.
const MAX_PACKAGE_BYTES: usize = 5 * 1024 * 1024;
/// The largest channel message (SPEC.md 6.9).
const MAX_CHANNEL_BYTES: usize = 256 * 1024;
/// How many channels keep a latest message.
const MAX_CHANNELS: usize = 500;

fn json_len(v: &Value) -> usize {
    serde_json::to_string(v).map(|s| s.len()).unwrap_or(usize::MAX)
}

/// A package (or app) name the state may be filed under.
pub fn check_name(name: &str) -> Result<(), String> {
    if safe_segment(name) {
        Ok(())
    } else {
        Err(format!("\"{name}\" is not an app name"))
    }
}

/// A layout to keep: an object no larger than the limit, or null to forget it.
pub fn check_layout(layout: &Value) -> Result<(), String> {
    if !(layout.is_null() || layout.is_object()) {
        return Err("a layout must be an object or null".into());
    }
    if json_len(layout) > MAX_LAYOUT_BYTES {
        return Err(format!("a layout may be at most {} KB", MAX_LAYOUT_BYTES / 1024));
    }
    Ok(())
}

/// Sets `key` of `app` in a package's data (null removes it), keeping within the limits.
/// `doc` is the package's data: {app: {key: value}}.
pub fn set_app_value(doc: &mut Map<String, Value>, app: &str, key: &str, value: Value) -> Result<(), String> {
    check_name(app)?;
    if key.is_empty() || key.len() > 200 {
        return Err("a storage key must be 1 to 200 characters".into());
    }
    let entry = doc.entry(app.to_string()).or_insert_with(|| Value::Object(Map::new()));
    let obj = entry.as_object_mut().ok_or("the stored data is damaged")?;
    if value.is_null() {
        obj.remove(key);
    } else {
        obj.insert(key.to_string(), value);
    }
    check_package(doc)
}

/// Adds the keys of `incoming` ({app: {key: value}}) that `doc` does not have yet: what a browser
/// held before the host kept it. Keys the host already has are kept as they are.
pub fn merge_missing(doc: &mut Map<String, Value>, incoming: &Value) -> Result<usize, String> {
    let incoming = incoming.as_object().ok_or("the data must be an object of apps")?;
    let mut added = 0;
    for (app, values) in incoming {
        check_name(app)?;
        let values = values.as_object().ok_or("each app's data must be an object")?;
        let entry = doc.entry(app.clone()).or_insert_with(|| Value::Object(Map::new()));
        let obj = entry.as_object_mut().ok_or("the stored data is damaged")?;
        for (k, v) in values {
            if !obj.contains_key(k) && !v.is_null() && !k.is_empty() && k.len() <= 200 {
                obj.insert(k.clone(), v.clone());
                added += 1;
            }
        }
    }
    check_package(doc)?;
    Ok(added)
}

fn check_package(doc: &Map<String, Value>) -> Result<(), String> {
    for (app, values) in doc {
        if json_len(values) > MAX_APP_BYTES {
            return Err(format!("{app} may keep at most {} KB", MAX_APP_BYTES / 1024));
        }
    }
    if json_len(&Value::Object(doc.clone())) > MAX_PACKAGE_BYTES {
        return Err(format!("an app may keep at most {} MB in all", MAX_PACKAGE_BYTES / (1024 * 1024)));
    }
    Ok(())
}

/// Keeps `message` as the latest on `channel` (null forgets it).
pub fn set_channel(all: &mut Map<String, Value>, channel: &str, message: Value) -> Result<(), String> {
    if !valid_channel(channel) {
        return Err(format!("\"{channel}\" is not a channel name"));
    }
    if message.is_null() {
        all.remove(channel);
        return Ok(());
    }
    if json_len(&message) > MAX_CHANNEL_BYTES {
        return Err(format!("a channel message may be at most {} KB", MAX_CHANNEL_BYTES / 1024));
    }
    if !all.contains_key(channel) && all.len() >= MAX_CHANNELS {
        return Err(format!("at most {MAX_CHANNELS} channels keep a latest message"));
    }
    all.insert(channel.to_string(), message);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn app_values_set_remove_and_keep_within_limits() {
        let mut doc = Map::new();
        set_app_value(&mut doc, "inputs", "state", json!({"d": "1,2"})).unwrap();
        assert_eq!(doc["inputs"]["state"]["d"], "1,2");
        set_app_value(&mut doc, "inputs", "state", Value::Null).unwrap();
        assert!(doc["inputs"].as_object().unwrap().is_empty());
        assert!(set_app_value(&mut doc, "../x", "k", json!(1)).is_err());
        assert!(set_app_value(&mut doc, "inputs", "big", json!("x".repeat(MAX_APP_BYTES + 1))).is_err());
    }

    #[test]
    fn merge_keeps_what_the_host_has() {
        let mut doc = Map::new();
        set_app_value(&mut doc, "inputs", "state", json!("host")).unwrap();
        let added = merge_missing(&mut doc, &json!({"inputs": {"state": "browser", "tableLink": true}, "chart": {"x": 1}})).unwrap();
        assert_eq!(added, 2);
        assert_eq!(doc["inputs"]["state"], "host");
        assert_eq!(doc["inputs"]["tableLink"], true);
    }

    /// ADR-2610071055 ¶2 (#5): a layout may be 20 KB, as JSON, and not one byte more.
    #[test]
    fn claim_a_layout_is_at_most_20_kb() {
        // {"x":"…"} is 8 bytes around the text.
        let layout = |n: usize| json!({ "x": "a".repeat(n - 8) });
        assert_eq!(json_len(&layout(20 * 1024)), 20 * 1024);
        assert!(check_layout(&layout(20 * 1024)).is_ok());
        assert!(check_layout(&layout(20 * 1024 + 1)).unwrap_err().contains("at most 20 KB"));
    }

    /// ADR-2610071055 ¶2 (#5): one package may keep 5 MB across its apps, as JSON.
    #[test]
    fn claim_a_package_keeps_at_most_5_mb() {
        let mut doc = Map::new();
        for i in 0..5 {
            set_app_value(&mut doc, &format!("a{i}"), "k", json!("x".repeat(1_000_000))).unwrap();
        }
        set_app_value(&mut doc, "last", "k", json!("")).unwrap();
        let room = 5 * 1024 * 1024 - json_len(&Value::Object(doc.clone()));
        set_app_value(&mut doc, "last", "k", json!("x".repeat(room))).unwrap();
        assert_eq!(json_len(&Value::Object(doc.clone())), 5 * 1024 * 1024, "exactly the limit is kept");
        let e = set_app_value(&mut doc, "last", "k", json!("x".repeat(room + 1))).unwrap_err();
        assert!(e.contains("at most 5 MB"), "{e}");
    }

    /// ADR-2610071055 ¶2, SPEC 6.9 (#5): a channel message may be 256 KB, as JSON.
    #[test]
    fn claim_a_channel_message_is_at_most_256_kb() {
        let mut all = Map::new();
        // A JSON string is 2 bytes more than its text.
        assert!(set_channel(&mut all, "c", json!("m".repeat(256 * 1024 - 2))).is_ok());
        assert!(set_channel(&mut all, "c", json!("m".repeat(256 * 1024 - 1))).unwrap_err().contains("at most 256 KB"));
    }

    /// ADR-2610071055 ¶2 (#5): at most 500 channels keep a latest message.
    #[test]
    fn claim_at_most_500_channels_are_kept() {
        let mut all = Map::new();
        for i in 0..500 {
            set_channel(&mut all, &format!("c{i}"), json!(i)).unwrap();
        }
        assert!(set_channel(&mut all, "c500", json!(1)).unwrap_err().contains("at most 500 channels"));
        assert!(set_channel(&mut all, "c7", json!("new")).is_ok(), "a kept channel may still change");
    }

    #[test]
    fn layouts_and_channels_are_checked() {
        assert!(check_layout(&json!({"v": 1})).is_ok() && check_layout(&Value::Null).is_ok());
        assert!(check_layout(&json!([1])).is_err());
        let mut all = Map::new();
        assert!(set_channel(&mut all, "splunk.table", json!({"rows": []})).is_ok());
        assert!(set_channel(&mut all, "Bad Name", json!(1)).is_err());
        set_channel(&mut all, "splunk.table", Value::Null).unwrap();
        assert!(all.is_empty());
    }
}
