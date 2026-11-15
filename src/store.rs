use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::applets::Kind;
use crate::settings::Prefs;

const FILE: &str = "state.json";

#[derive(Serialize, Deserialize)]
pub struct AppletSave {
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Serialize, Deserialize)]
pub struct TileSave {
    pub id: u64,
    pub cell: [usize; 4],
    pub active: usize,
    pub applets: Vec<AppletSave>,
}

#[derive(Serialize, Deserialize)]
pub struct Saved {
    pub prefs: Prefs,
    pub tiles: Vec<TileSave>,
    pub next_id: u64,
}

pub struct Store {
    dir: PathBuf,
    last: String,
}

impl Store {
    pub fn open() -> Self {
        let dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tilezone");
        if let Err(e) = fs::create_dir_all(&dir) {
            eprintln!("store: {e}");
        }
        Self { dir, last: String::new() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn load(&mut self) -> Option<Saved> {
        let path = self.dir.join(FILE);
        let text = fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&text) {
            Ok(saved) => {
                self.last = text;
                Some(saved)
            }
            Err(e) => {
                eprintln!("store: {e}");
                let _ = fs::copy(&path, self.dir.join("state.json.bad"));
                None
            }
        }
    }

    pub fn save(&mut self, saved: &Saved) {
        let Ok(text) = serde_json::to_string_pretty(saved) else { return };
        if text == self.last {
            return;
        }
        let tmp = self.dir.join("state.json.tmp");
        if fs::write(&tmp, &text).and_then(|_| fs::rename(&tmp, self.dir.join(FILE))).is_err() {
            return;
        }
        self.last = text;
        self.prune();
    }

    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("paint_") && name.ends_with(".png") && !self.last.contains(&name) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trip() {
        let saved = Saved {
            prefs: Prefs::default(),
            tiles: vec![TileSave {
                id: 2,
                cell: [0, 1, 2, 2],
                active: 1,
                applets: vec![AppletSave { kind: Kind::Todo, data: Some(json!({ "items": [[false, "milk"]] })) }],
            }],
            next_id: 6,
        };
        let text = serde_json::to_string(&saved).unwrap();
        let back: Saved = serde_json::from_str(&text).unwrap();
        assert_eq!(back.tiles[0].applets[0].kind, Kind::Todo);
        assert_eq!(back.next_id, 6);
        let partial: Saved = serde_json::from_str(r#"{"prefs":{"zoom":1.2},"tiles":[],"next_id":1}"#).unwrap();
        assert_eq!(partial.prefs.zoom, 1.2);
        assert_eq!(partial.prefs.gap, 12.0);
    }
}
