//! Folders in the app list (ADR-2610081830): how the viewer files the apps being served. One level
//! only; an app is in at most one folder. This module holds the record, what a valid record is,
//! the tidying against the apps being served, and the one-time filing of the example apps.
//! Reading and writing the file is the use case's business.

use super::package::safe_segment;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// The most folders a viewer may have.
const MAX_FOLDERS: usize = 100;
/// The longest folder name, in characters.
const MAX_NAME_CHARS: usize = 60;
/// The longest folder id.
const MAX_ID_CHARS: usize = 40;
/// The id of the folder the example apps are filed in.
const EXAMPLES_ID: &str = "examples";
/// Its name when it is made.
const EXAMPLES_NAME: &str = "Examples";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub apps: Vec<String>,
}

/// `state/folders.json`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Folders {
    pub v: u32,
    pub folders: Vec<Folder>,
    /// The example apps have been filed once.
    #[serde(default)]
    pub seeded: bool,
    /// The example apps already offered to the folders, so one the viewer moved out stays out and
    /// only an example new to this data folder is filed in "Examples".
    #[serde(default)]
    pub examples: Vec<String>,
}

impl Default for Folders {
    fn default() -> Self {
        Folders { v: 1, folders: Vec::new(), seeded: false, examples: Vec::new() }
    }
}

impl Folders {
    /// The record as kept; anything unreadable is an empty record that has not been seeded.
    pub fn read(v: &Value) -> Folders {
        serde_json::from_value::<Folders>(v.clone()).ok().filter(|f| check(f).is_ok()).unwrap_or_default()
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// The folder `app` is in, if any.
    pub fn folder_of(&self, app: &str) -> Option<&str> {
        self.folders.iter().find(|f| f.apps.iter().any(|a| a == app)).map(|f| f.id.as_str())
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_CHARS && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A folder name to keep: trimmed, 1 to 60 characters, no control characters.
pub fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let n = name.chars().count();
    if n == 0 || n > MAX_NAME_CHARS {
        return Err(format!("a folder name must be 1 to {MAX_NAME_CHARS} characters"));
    }
    if name.chars().any(char::is_control) {
        return Err("a folder name may not hold control characters".into());
    }
    Ok(name.to_string())
}

/// What makes a record valid: at most 100 folders, each with its own id and a name of 1 to 60
/// characters, holding app names, and no app in two folders.
pub fn check(f: &Folders) -> Result<(), String> {
    if f.folders.len() > MAX_FOLDERS {
        return Err(format!("at most {MAX_FOLDERS} folders"));
    }
    let (mut ids, mut apps) = (HashSet::new(), HashSet::new());
    for folder in &f.folders {
        if !valid_id(&folder.id) {
            return Err(format!("\"{}\" is not a folder id", folder.id));
        }
        if !ids.insert(folder.id.as_str()) {
            return Err(format!("two folders have the id \"{}\"", folder.id));
        }
        if check_name(&folder.name)? != folder.name {
            return Err("a folder name may not start or end with spaces".into());
        }
        for app in &folder.apps {
            if !safe_segment(app) {
                return Err(format!("\"{app}\" is not an app name"));
            }
            if !apps.insert(app.as_str()) {
                return Err(format!("{app} is in two folders"));
            }
        }
    }
    Ok(())
}

/// The record a viewer sends, `{folders: [...]}`, checked, with its names trimmed. What only the
/// host decides (`seeded`, `examples`) comes from `kept`, never from the viewer.
pub fn from_viewer(body: &Value, kept: &Folders) -> Result<Folders, String> {
    let list = body.get("folders").ok_or("send {\"folders\": [...]}")?;
    let mut folders: Vec<Folder> = serde_json::from_value(list.clone()).map_err(|e| format!("the folders are not readable: {e}"))?;
    for f in &mut folders {
        f.name = check_name(&f.name)?;
    }
    let out = Folders { v: 1, folders, seeded: kept.seeded, examples: kept.examples.clone() };
    check(&out)?;
    Ok(out)
}

/// Drops every name in a folder that is not one of `apps` (the apps being served). Returns whether
/// anything changed. An empty list changes nothing: no apps is more likely a source that has not
/// answered yet than every app removed.
pub fn tidy(f: &mut Folders, apps: &[String]) -> bool {
    if apps.is_empty() {
        return false;
    }
    let served: HashSet<&str> = apps.iter().map(String::as_str).collect();
    let mut changed = false;
    for folder in &mut f.folders {
        let before = folder.apps.len();
        folder.apps.retain(|a| served.contains(a.as_str()));
        changed |= folder.apps.len() != before;
    }
    changed
}

/// Files the example apps: the first time, every example among `apps` that is in no folder goes
/// into a folder named "Examples", open only when the viewer has no app of their own. After that,
/// an example new to this data folder goes there too while that folder exists. Returns whether
/// anything changed. Waits for a list with apps in it, like `tidy`.
pub fn seed(f: &mut Folders, apps: &[String], examples: &[String]) -> bool {
    if apps.is_empty() {
        return false;
    }
    let is_example = |a: &String| examples.contains(a);
    let present: Vec<&String> = apps.iter().filter(|a| is_example(a)).collect();
    let new: Vec<String> = if f.seeded {
        present.iter().filter(|a| !f.examples.contains(a)).map(|a| (*a).clone()).collect()
    } else {
        present.iter().map(|a| (*a).clone()).collect()
    };
    let changed = !f.seeded || !new.is_empty();
    let to_file: Vec<String> = new.iter().filter(|a| f.folder_of(a).is_none()).cloned().collect();
    if !to_file.is_empty() {
        let existing = f.folders.iter().position(|x| x.id == EXAMPLES_ID);
        let at = match existing {
            Some(i) => Some(i),
            // Made only at the first filing; once the viewer deletes it, it stays deleted.
            None if !f.seeded && f.folders.len() < MAX_FOLDERS => {
                let own = apps.iter().any(|a| !is_example(a));
                f.folders.push(Folder { id: EXAMPLES_ID.into(), name: EXAMPLES_NAME.into(), open: !own, apps: Vec::new() });
                Some(f.folders.len() - 1)
            }
            None => None,
        };
        if let Some(i) = at {
            let folder = &mut f.folders[i];
            folder.apps.extend(to_file);
            folder.apps.sort();
        }
    }
    f.seeded = true;
    f.examples.extend(new);
    f.examples.retain(|a| examples.contains(a));
    f.examples.sort();
    f.examples.dedup();
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn folder(id: &str, name: &str, apps: &[&str]) -> Folder {
        Folder { id: id.into(), name: name.into(), open: false, apps: names(apps) }
    }
    fn record(folders: Vec<Folder>) -> Folders {
        Folders { folders, ..Folders::default() }
    }

    /// ADR-2610081830 ¶5: a folder name is 1 to 60 characters.
    #[test]
    fn folders_name_is_1_to_60_characters() {
        assert_eq!(check_name("  Work  ").unwrap(), "Work");
        assert!(check_name("").is_err() && check_name("   ").is_err());
        assert!(check_name(&"é".repeat(60)).is_ok(), "characters, not bytes");
        assert!(check_name(&"a".repeat(61)).unwrap_err().contains("1 to 60"));
        assert!(check_name("a\nb").is_err());
    }

    /// ADR-2610081830 ¶5: up to 100 folders.
    #[test]
    fn folders_at_most_100() {
        let many = |n: usize| record((0..n).map(|i| folder(&format!("f{i}"), "F", &[])).collect());
        assert!(check(&many(100)).is_ok());
        assert!(check(&many(101)).unwrap_err().contains("at most 100"));
    }

    #[test]
    fn folders_ids_are_unique_and_apps_in_one_folder() {
        assert!(check(&record(vec![folder("a", "A", &[]), folder("a", "B", &[])])).unwrap_err().contains("two folders have the id"));
        assert!(check(&record(vec![folder("bad id", "A", &[])])).is_err());
        assert!(check(&record(vec![folder("a", "A", &["x"]), folder("b", "B", &["x"])])).unwrap_err().contains("in two folders"));
        assert!(check(&record(vec![folder("a", "A", &["../x"])])).is_err());
    }

    #[test]
    fn folders_from_viewer_trims_and_keeps_host_fields() {
        let kept = Folders { seeded: true, examples: names(&["adder"]), ..Folders::default() };
        let body = json!({"folders": [{"id": "f1", "name": "  Work ", "open": true, "apps": ["a"]}], "seeded": false, "examples": []});
        let f = from_viewer(&body, &kept).unwrap();
        assert_eq!(f.folders[0].name, "Work");
        assert!(f.seeded && f.examples == names(&["adder"]), "the viewer cannot reset seeding");
        assert!(from_viewer(&json!({"folders": [{"id": "f1", "name": ""}]}), &kept).is_err());
        assert!(from_viewer(&json!({}), &kept).is_err());
    }

    #[test]
    fn folders_unreadable_record_reads_as_empty() {
        assert_eq!(Folders::read(&json!("nonsense")), Folders::default());
        assert_eq!(Folders::read(&json!({"v": 1, "folders": [{"id": "a", "name": ""}]})), Folders::default());
        let ok = json!({"v": 1, "folders": [{"id": "a", "name": "A", "open": true, "apps": ["x"]}], "seeded": true});
        assert!(Folders::read(&ok).seeded);
    }

    /// ADR-2610081830 ¶5: a name that no longer matches an app is dropped; the folder stays.
    #[test]
    fn folders_tidy_drops_unknown_apps() {
        let mut f = record(vec![folder("a", "A", &["kept", "gone"]), folder("b", "B", &["also-gone"])]);
        assert!(tidy(&mut f, &names(&["kept", "new"])));
        assert_eq!(f.folders[0].apps, names(&["kept"]));
        assert!(f.folders[1].apps.is_empty() && f.folders.len() == 2, "an empty folder is kept");
        assert!(!tidy(&mut f, &names(&["kept", "new"])), "nothing more to drop");
        assert_eq!(f.folder_of("new"), None, "a new app is in no folder");
    }

    #[test]
    fn folders_empty_app_list_changes_nothing() {
        let mut f = record(vec![folder("a", "A", &["kept"])]);
        assert!(!tidy(&mut f, &[]) && f.folders[0].apps == names(&["kept"]));
        let mut fresh = Folders::default();
        assert!(!seed(&mut fresh, &[], &names(&["adder"])) && !fresh.seeded, "seeding waits for the list");
    }

    /// ADR-2610081830 ¶3: examples go into "Examples" once, closed when the viewer has apps of
    /// their own, open when not.
    #[test]
    fn folders_seeding_happens_once() {
        let examples = names(&["adder", "usl-lab", "not-here"]);
        let mut f = Folders::default();
        assert!(seed(&mut f, &names(&["adder", "mine", "usl-lab"]), &examples));
        assert_eq!(f.folders, vec![Folder { id: "examples".into(), name: "Examples".into(), open: false, apps: names(&["adder", "usl-lab"]) }]);
        assert!(f.seeded);
        // The viewer moves one out and deletes nothing: seeding again changes nothing.
        f.folders[0].apps.retain(|a| a != "adder");
        assert!(!seed(&mut f, &names(&["adder", "mine", "usl-lab"]), &examples));
        assert_eq!(f.folder_of("adder"), None);

        let mut only = Folders::default();
        seed(&mut only, &names(&["adder"]), &examples);
        assert!(only.folders[0].open, "open when every app is an example");

        let mut none = Folders::default();
        assert!(seed(&mut none, &names(&["mine"]), &examples));
        assert!(none.folders.is_empty() && none.seeded, "no example served: no folder, but seeded");
    }

    #[test]
    fn folders_seeding_leaves_filed_examples_where_they_are() {
        let mut f = record(vec![folder("w", "Work", &["adder"])]);
        seed(&mut f, &names(&["adder", "usl-lab"]), &names(&["adder", "usl-lab"]));
        assert_eq!(f.folder_of("adder"), Some("w"));
        assert_eq!(f.folder_of("usl-lab"), Some("examples"));
    }

    /// ADR-2610081830 ¶3: an example added later goes into "Examples" while that folder exists.
    #[test]
    fn folders_later_example_goes_into_examples() {
        let examples = names(&["adder", "chart", "dice"]);
        let mut f = Folders::default();
        seed(&mut f, &names(&["adder"]), &examples);
        assert!(seed(&mut f, &names(&["adder", "chart"]), &examples));
        assert_eq!(f.folders[0].apps, names(&["adder", "chart"]));
        // Once the viewer deletes the folder, a later example is in no folder.
        f.folders.clear();
        assert!(seed(&mut f, &names(&["adder", "chart", "dice"]), &examples));
        assert!(f.folders.is_empty() && f.folder_of("dice").is_none());
        assert!(!seed(&mut f, &names(&["adder", "chart", "dice"]), &examples));
    }
}
