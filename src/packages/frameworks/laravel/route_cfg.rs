//! Konfig-vezérelt route-ok (H24): a `foreach (config('a.b') as $k => $v) Route::get($v['uri'], …)` ciklus akkor is
//! kibontható, ha a tömb a `config/a.php`-ban literál — a ciklus változói a tömb értékeire cserélődnek (PHP-forrásként),
//! és a `routes.rs` a kibontott láncot ugyanúgy olvassa, mint a kézzel írtat.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::super::files::ProjectFiles;
use crate::core::phpval::{self, Entry, Value};
use crate::model::Imports;

/// A projekt `config/*.php` fájljainak literál tömbjei: fájlnév → (elemek, a fájl `use`-ai).
#[derive(Debug, Default)]
pub(super) struct Configs {
    files: BTreeMap<String, (Vec<Entry>, Imports)>,
}

impl Configs {
    /// A `<root>/config/*.php` fájlok beolvasása (csak a közvetlen gyerekek; a nem `return [..]` fájl kimarad).
    pub(super) fn load(root: &Path, files: &dyn ProjectFiles) -> Self {
        let dir = root.join("config");
        let mut out = Self::default();
        for path in files.files_under(&dir) {
            if path.extension().is_none_or(|ext| ext != "php") {
                continue;
            }
            let direct = path
                .strip_prefix(&dir)
                .is_ok_and(|rest| rest.components().count() == 1);
            let stem = path.file_stem().and_then(|stem| stem.to_str());
            let (true, Some(stem)) = (direct, stem) else {
                continue;
            };
            let Some(text) = files.read(&path) else {
                continue;
            };
            let Some(entries) = phpval::parse_return(&text) else {
                continue;
            };
            let imports = super::super::targeted::imports_only(&path, &text).imports;
            out.files.insert(stem.to_owned(), (entries, imports));
        }
        out
    }

    /// Van-e beolvasott konfig-fájl.
    pub(super) fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// A `a.b.c` konfig-kulcs tömbje (az `a` fájl `b.c` kulcsa): a tömb elemei és a fájl `use`-ai; nem tömb /
    /// ismeretlen kulcs: `None`.
    pub(super) fn lookup(&self, path: &str) -> Option<(&[Entry], &Imports)> {
        let mut parts = path.split('.');
        let (entries, imports) = self.files.get(parts.next()?)?;
        let mut current: &[Entry] = entries;
        for key in parts {
            current = current
                .iter()
                .find(|entry| entry.key == key)?
                .value
                .entries()?;
        }
        Some((current, imports))
    }
}

/// A ciklus változói: név (`$` nélkül) → érték (az osztálynevek teljesek).
#[derive(Debug, Clone, Default)]
pub(super) struct Env {
    vars: Vec<(String, Value)>,
}

impl Env {
    /// A ciklus egy eleme: a kulcs-változó (ha van) és az érték-változó.
    pub(super) fn of(
        key_var: Option<&str>,
        value_var: &str,
        entry: &Entry,
        imports: &Imports,
    ) -> Self {
        let mut vars = Vec::new();
        if let Some(name) = key_var {
            let key = entry
                .key
                .parse::<i64>()
                .map_or_else(|_| Value::Str(entry.key.clone()), Value::Int);
            vars.push((name.to_owned(), key));
        }
        vars.push((value_var.to_owned(), resolve_classes(&entry.value, imports)));
        Self { vars }
    }

    /// A külső ciklus változóival együtt.
    pub(super) fn merged(&self, outer: Option<&Env>) -> Env {
        let mut vars = outer.map(|env| env.vars.clone()).unwrap_or_default();
        vars.extend(self.vars.iter().cloned());
        Env { vars }
    }

    /// A `$változó` / `$változó['kulcs']['kulcs']` használatok a tömb-literál értékeire cserélve (PHP-forrásként);
    /// a hiányzó kulcs `null`; az ismeretlen változó érintetlen.
    pub(super) fn subst(&self, text: &str) -> String {
        static VAR: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"\$(\w+)((?:\s*\[\s*(?:'[^']*'|"[^"]*"|\d+)\s*\])*)"#)
                .expect("érvényes regex")
        });
        static KEY: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"\[\s*(?:'([^']*)'|"([^"]*)"|(\d+))\s*\]"#).expect("érvényes regex")
        });
        VAR.replace_all(text, |caps: &regex::Captures| {
            let Some((_, root)) = self.vars.iter().find(|(name, _)| name == &caps[1]) else {
                return caps[0].to_owned();
            };
            let mut value = Some(root);
            for key in KEY.captures_iter(&caps[2]) {
                let key = key
                    .get(1)
                    .or_else(|| key.get(2))
                    .or_else(|| key.get(3))
                    .map_or("", |m| m.as_str());
                value = value.and_then(|value| value.get(key));
            }
            value.map_or_else(|| "null".to_owned(), |value| value.to_php(&str::to_owned))
        })
        .into_owned()
    }

    /// A konfig-elem Inertia-komponense (`component` / `page` kulcs), ha van.
    pub(super) fn page(&self) -> Option<String> {
        self.vars.iter().find_map(|(_, value)| {
            ["component", "page"].iter().find_map(|key| {
                value
                    .get(key)?
                    .as_str()
                    .filter(|page| !page.is_empty())
                    .map(str::to_owned)
            })
        })
    }
}

/// Az osztálynevek teljessé tétele a fájl `use`-aival (a konfig-elem értékében).
fn resolve_classes(value: &Value, imports: &Imports) -> Value {
    match value {
        Value::Class(name) => Value::Class(resolve_name(imports, name)),
        Value::Array(entries) => Value::Array(
            entries
                .iter()
                .map(|entry| Entry {
                    key: entry.key.clone(),
                    value: resolve_classes(&entry.value, imports),
                    line: entry.line,
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Az osztálynév teljes alakja az import-térképből (a vezető `\` = már teljes).
pub(super) fn resolve_name(imports: &Imports, raw: &str) -> String {
    let raw = raw.trim();
    if let Some(full) = raw.strip_prefix('\\') {
        return full.to_owned();
    }
    let (head, rest) = match raw.split_once('\\') {
        Some((head, rest)) => (head, Some(rest)),
        None => (raw, None),
    };
    match (imports.get(head), rest) {
        (Some(full), Some(rest)) => format!("{full}\\{rest}"),
        (Some(full), None) => full.to_owned(),
        (None, _) => raw.to_owned(),
    }
}

/// Egy `foreach`, amelynek forrása literál konfig-tömb: a törzs bájt-tartománya és az elemenkénti változók.
pub(super) struct Loop {
    pub body: (usize, usize),
    pub envs: Vec<Env>,
}
