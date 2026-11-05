//! Projekt-registry (`projects.json`): alias → abszolút gyökér, vagy `{ "path": …, <projektenkénti beállítás> }`.
//! A kulcsok beszúrási sorrendje megmarad (serde_json `preserve_order`), a kiírás a jq-éval azonos (2 szóközös behúzás).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{CallgraphError, Result};

pub struct ProjectRegistry {
    file: PathBuf,
    map: Map<String, Value>,
}

impl ProjectRegistry {
    /// Beolvassa; hiányzó fájl üres registry, hibás JSON használati hiba.
    pub fn load(file: &Path) -> Result<Self> {
        let map = match std::fs::read_to_string(file) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(Value::Object(map)) => map,
                _ => {
                    return Err(CallgraphError::Usage(crate::tf!(
                        "projects.json is not a valid JSON object: {}",
                        "A projects.json nem érvényes JSON-objektum: {}",
                        file.display()
                    )));
                }
            },
            Err(_) => Map::new(),
        };
        Ok(Self {
            file: file.to_path_buf(),
            map,
        })
    }

    /// Létrehozza az üres (`{}`) fájlt, ha még nincs (a régi `projects.sh` viselkedése).
    pub fn ensure_file(file: &Path) -> Result<()> {
        if !file.exists() {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(file, "{}\n")?;
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let text = serde_json::to_string_pretty(&self.map)? + "\n";
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.file)?;
        Ok(())
    }

    pub fn contains(&self, alias: &str) -> bool {
        self.map.contains_key(alias)
    }

    /// Felveszi vagy felülírja (sima string-bejegyzésként, mint a `jq '.[$a] = $p'`).
    pub fn add(&mut self, alias: &str, path: &str) {
        self.map
            .insert(alias.to_owned(), Value::String(path.to_owned()));
    }

    /// Törli; `false`, ha nem volt ilyen.
    pub fn remove(&mut self, alias: &str) -> bool {
        self.map.shift_remove(alias).is_some()
    }

    /// Az alias gyökér-útja (string vagy `path` mező); `None`, ha nincs vagy üres.
    pub fn root(&self, alias: &str) -> Option<String> {
        let path = match self.map.get(alias)? {
            Value::String(s) => s.as_str(),
            Value::Object(o) => o.get("path")?.as_str()?,
            _ => return None,
        };
        Some(path.to_owned()).filter(|p| !p.is_empty())
    }

    /// Egy projektenkénti extra beállítás (pl. `phpstan_level`); string-bejegyzésnél nincs.
    pub fn setting(&self, alias: &str, key: &str) -> Option<String> {
        match self.map.get(alias)?.get(key)? {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        }
    }

    /// Beállítja a PHPStan-szintet / -configot; a string-bejegyzést előbb `{path}` objektummá alakítja.
    pub fn set_config(
        &mut self,
        alias: &str,
        level: Option<&str>,
        config: Option<&str>,
    ) -> Option<&Value> {
        let entry = self.map.get_mut(alias)?;
        if !entry.is_object() {
            let mut o = Map::new();
            o.insert("path".into(), entry.take());
            *entry = Value::Object(o);
        }
        let o = entry.as_object_mut()?;
        for (key, value) in [("phpstan_level", level), ("phpstan_config", config)] {
            if let Some(v) = value {
                o.insert(key.into(), Value::String(v.to_owned()));
            }
        }
        self.map.get(alias)
    }

    /// `alias<TAB>út[  [phpstan_level=… phpstan_config=…]]` — a `projects.sh list` sorai.
    pub fn list_lines(&self) -> Vec<String> {
        self.map
            .iter()
            .map(|(alias, v)| match v {
                Value::Object(o) => {
                    let get = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("-").to_owned();
                    let path = o.get("path").and_then(Value::as_str).unwrap_or("");
                    format!(
                        "{alias}\t{path}  [phpstan_level={} phpstan_config={}]",
                        get("phpstan_level"),
                        get("phpstan_config")
                    )
                }
                Value::String(s) => format!("{alias}\t{s}"),
                other => format!("{alias}\t{other}"),
            })
            .collect()
    }
}
