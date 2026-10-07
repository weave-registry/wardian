//! Keeps the viewer's state in the data folder (ADR-2610071055): Arrange layouts, each suite app's
//! saved data, and the latest message per channel, under `<data dir>/state/`. Every file is
//! written private, like the keys and the permission answers.

use crate::domain::viewer_state::{self, check_layout, check_name};
use crate::ports::{service::ViewerState, storage::FileSystem};
use serde_json::{json, Map, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct State {
    fs: Arc<dyn FileSystem>,
    dir: PathBuf,
    /// One writer at a time, so two changes never overwrite each other.
    lock: Mutex<()>,
}

impl State {
    pub fn new(fs: Arc<dyn FileSystem>, data_dir: &Path) -> State {
        State { fs, dir: data_dir.join("state"), lock: Mutex::new(()) }
    }

    fn read_map(&self, path: &Path) -> Map<String, Value> {
        self.fs.read(path).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
    }

    fn write_map(&self, path: &Path, map: &Map<String, Value>) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            self.fs.create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec(&Value::Object(map.clone())).map_err(|e| e.to_string())?;
        self.fs.write_private(path, &bytes).map_err(|e| format!("saving {}: {e}", path.display()))
    }

    fn layouts_path(&self) -> PathBuf {
        self.dir.join("layouts.json")
    }
    fn channels_path(&self) -> PathBuf {
        self.dir.join("channels.json")
    }
    fn app_path(&self, package: &str) -> PathBuf {
        self.dir.join("apps").join(format!("{package}.json"))
    }
}

impl ViewerState for State {
    fn layout(&self, package: &str) -> Result<Value, String> {
        check_name(package)?;
        Ok(self.read_map(&self.layouts_path()).remove(package).unwrap_or(Value::Null))
    }

    fn set_layout(&self, package: &str, layout: Value) -> Result<Value, String> {
        check_name(package)?;
        check_layout(&layout)?;
        let _guard = self.lock.lock().unwrap();
        let mut all = self.read_map(&self.layouts_path());
        if layout.is_null() {
            all.remove(package);
        } else {
            all.insert(package.to_string(), layout);
        }
        self.write_map(&self.layouts_path(), &all)?;
        Ok(json!({ "saved": true }))
    }

    fn app_data(&self, package: &str) -> Result<Value, String> {
        check_name(package)?;
        Ok(Value::Object(self.read_map(&self.app_path(package))))
    }

    fn set_app_value(&self, package: &str, app: &str, key: &str, value: Value) -> Result<Value, String> {
        check_name(package)?;
        let _guard = self.lock.lock().unwrap();
        let path = self.app_path(package);
        let mut doc = self.read_map(&path);
        viewer_state::set_app_value(&mut doc, app, key, value)?;
        self.write_map(&path, &doc)?;
        Ok(json!({ "saved": true }))
    }

    fn merge_app_data(&self, package: &str, data: &Value) -> Result<Value, String> {
        check_name(package)?;
        let _guard = self.lock.lock().unwrap();
        let path = self.app_path(package);
        let mut doc = self.read_map(&path);
        let added = viewer_state::merge_missing(&mut doc, data)?;
        if added > 0 {
            self.write_map(&path, &doc)?;
        }
        Ok(json!({ "added": added, "data": Value::Object(doc) }))
    }

    fn channel(&self, channel: &str) -> Value {
        self.read_map(&self.channels_path()).remove(channel).unwrap_or(Value::Null)
    }

    fn set_channel(&self, channel: &str, message: Value) -> Result<Value, String> {
        let _guard = self.lock.lock().unwrap();
        let mut all = self.read_map(&self.channels_path());
        viewer_state::set_channel(&mut all, channel, message)?;
        self.write_map(&self.channels_path(), &all)?;
        Ok(json!({ "saved": true }))
    }
}
