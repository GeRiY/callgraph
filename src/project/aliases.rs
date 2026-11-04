//! Projekt-aliasok forrása: a callgraph `projects.json` (opcionális).

use std::path::{Path, PathBuf};

/// Alias-forrás: gyökér-útból aliast ad. Cserélhető (a tesztek és a locator ezen át függenek tőle).
pub trait AliasSource {
    /// A gyökérhez tartozó alias, ha ismert.
    fn alias_for(&self, root: &Path) -> Option<String>;
}

/// Nincs alias (alapértelmezett, ha a `projects.json` hiányzik).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoAliases;

impl AliasSource for NoAliases {
    fn alias_for(&self, _root: &Path) -> Option<String> {
        None
    }
}

/// A `projects.json` tartalma: alias → (kanonizált) gyökér-út.
#[derive(Debug, Default, Clone)]
pub struct ProjectsJsonAliases {
    entries: Vec<(String, PathBuf)>,
}

impl ProjectsJsonAliases {
    /// Az alapértelmezett hely: `~/.claude/skills/callgraph/projects.json`; ha nem létezik/olvasható, üres.
    pub fn from_default_location() -> Self {
        match std::env::var_os("HOME") {
            Some(home) => {
                Self::from_file(&Path::new(&home).join(".claude/skills/callgraph/projects.json"))
            }
            None => Self::default(),
        }
    }

    /// Beolvassa a fájlt; hiányzó vagy hibás fájlra üres készlet.
    pub fn from_file(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Értelmezi a JSON-t: érték vagy egy út-string, vagy `{ "path": "…" }` objektum.
    pub fn parse(text: &str) -> Self {
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str(text) else {
            return Self::default();
        };
        let entries = map
            .into_iter()
            .filter_map(|(alias, v)| {
                let path = match &v {
                    serde_json::Value::String(s) => s.as_str(),
                    serde_json::Value::Object(o) => o.get("path")?.as_str()?,
                    _ => return None,
                };
                let path = PathBuf::from(path);
                Some((alias, path.canonicalize().unwrap_or(path)))
            })
            .collect();
        Self { entries }
    }
}

impl AliasSource for ProjectsJsonAliases {
    fn alias_for(&self, root: &Path) -> Option<String> {
        self.entries
            .iter()
            .find(|(_, p)| p == root)
            .map(|(alias, _)| alias.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_and_object_values() {
        let a = ProjectsJsonAliases::parse(
            r#"{"a": "/nincs/a", "b": {"path": "/nincs/b", "phpstan_level": "3"}, "c": 5}"#,
        );
        assert_eq!(a.alias_for(Path::new("/nincs/a")).as_deref(), Some("a"));
        assert_eq!(a.alias_for(Path::new("/nincs/b")).as_deref(), Some("b"));
        assert_eq!(a.alias_for(Path::new("/nincs/c")), None);
    }

    #[test]
    fn missing_or_invalid_file_is_empty() {
        assert!(
            ProjectsJsonAliases::from_file(Path::new("/nincs/ilyen.json"))
                .entries
                .is_empty()
        );
        assert!(ProjectsJsonAliases::parse("nem json").entries.is_empty());
    }
}
