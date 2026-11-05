//! A teljes Rust-csővezeték tesztjei (parser + visitorok → `FileGraph`).

use super::RustBackend;
use crate::lang::{Language, LanguageRole};
use crate::model::FileGraph;

const SRC: &str = r#"//! Users API.

use std::collections::HashMap;
use std::fmt::{self, Display as Show};
use crate::db::{Repo, connect as open_db};
use axum::Json;

/// Max users.
pub const MAX: usize = 10;
static mut COUNTER: u32 = 0;

/// A user.
#[derive(Debug)]
pub struct User {
    /// Id.
    pub id: u64,
    name: String,
}

pub enum Role {
    Admin = 1,
    Guest,
}

impl User {
    /// Creates a user.
    pub fn new(id: u64) -> Self {
        let user = Self { id, name: String::new() };
        user.check();
        user
    }

    pub(crate) async fn save(&self, repo: &Repo) -> Result<(), String> {
        // TODO: retry on failure
        if self.id == 0 {
            return Err(format!("bad id {}", self.id));
        } else if self.id > 10 {
            println!("{}", helper(self.id));
        } else {
            self.check();
        }
        repo.store(&self.name).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn check(&self) {
        match self.id {
            0 => {}
            n if n > 5 => open_db(),
            _ => unreachable!(),
        }
    }
}

impl Show for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

pub trait Store: Send {
    fn put(&self, key: &str);
    fn get(&self) -> u8 {
        0
    }
}

fn helper(x: u64) -> u64 {
    let cb = |v: u64| v + 1;
    let map: HashMap<u64, u64> = HashMap::new();
    Json(cb(x) + map.len() as u64).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        assert_eq!(helper(1), 2);
    }
}
"#;

fn graph() -> FileGraph {
    RustBackend::with_defaults()
        .analyze_source("/p/src/api/users.rs", SRC.as_bytes())
        .graph
}

#[test]
fn language_trait_reports_rust_backend() {
    let b = RustBackend::with_defaults();
    assert_eq!(
        (b.id(), b.role(), b.extensions()),
        ("rust", LanguageRole::Backend, &["rs"][..])
    );
}

#[test]
fn containers_with_kinds_lines_and_traits() {
    let g = graph();
    let c: Vec<(&str, &str, u32, u32)> = g
        .containers
        .iter()
        .map(|c| (c.kind.as_str(), c.name.as_str(), c.line, c.end_line))
        .collect();
    assert_eq!(
        c,
        [
            ("module", "users", 1, 82),
            ("struct", "User", 14, 18),
            ("enum", "Role", 20, 23),
            ("trait", "Store", 61, 66),
            ("mod", "tests", 75, 82),
        ]
    );
    assert_eq!(g.containers[0].doc.as_deref(), Some("Users API."));
    assert_eq!(g.containers[1].doc.as_deref(), Some("A user."));
    assert_eq!(g.containers[1].implements, Some(vec!["Show".into()]));
    assert_eq!(g.containers[3].extends, Some(vec!["Send".into()]));
}

#[test]
fn methods_have_visibility_kind_params_and_ranges() {
    let g = graph();
    let find = |n: &str| g.methods.iter().find(|m| m.name == n).unwrap();
    let new = find("new");
    assert_eq!(new.container.as_deref(), Some("User"));
    assert_eq!((new.line, new.end_line), (27, Some(31)));
    assert_eq!(new.visibility.as_deref(), Some("pub"));
    assert!(new.is_static);
    assert_eq!(new.params, ["id: u64"]);
    assert_eq!(new.return_type.as_deref(), Some("Self"));
    assert_eq!(new.doc.as_deref(), Some("Creates a user."));
    let save = find("save");
    assert_eq!(save.kind.as_deref(), Some("async method"));
    assert_eq!(save.visibility.as_deref(), Some("pub(crate)"));
    assert_eq!(save.params, ["&self", "repo: &Repo"]);
    assert!(find("put").is_abstract);
    assert_eq!(find("fmt").visibility.as_deref(), Some("pub"));
    assert_eq!(find("helper").container.as_deref(), Some("users"));
    assert_eq!(find("works").container.as_deref(), Some("tests"));
}

#[test]
fn properties_constants_and_imports() {
    let g = graph();
    let props: Vec<(&str, Option<&str>, &str)> = g
        .properties
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.type_hint.as_deref(),
                p.visibility.as_str(),
            )
        })
        .collect();
    assert_eq!(
        props,
        [
            ("id", Some("u64"), "pub"),
            ("name", Some("String"), "private")
        ]
    );
    assert_eq!(g.properties[0].line, 16);
    let consts: Vec<(&str, &str, Option<&str>)> = g
        .constants
        .iter()
        .map(|c| (c.kind.as_str(), c.name.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(
        consts,
        [
            ("const", "MAX", Some("10")),
            ("static mut", "COUNTER", Some("0")),
            ("enum_case", "Admin", Some("1")),
            ("enum_case", "Guest", None),
        ]
    );
    assert_eq!(g.imports.get("HashMap"), Some("std::collections::HashMap"));
    assert_eq!(g.imports.get("fmt"), Some("std::fmt"));
    assert_eq!(g.imports.get("Show"), Some("std::fmt::Display"));
    assert_eq!(g.imports.get("open_db"), Some("crate::db::connect"));
    assert_eq!(g.imports.get("super::*"), Some("super::*"));
}

pub(super) fn calls_of(g: &FileGraph, caller: &str) -> Vec<(String, String, Option<String>)> {
    g.calls
        .iter()
        .filter(|c| c.caller.as_deref() == Some(caller))
        .map(|c| (c.target.clone(), c.category.clone(), c.resolved.clone()))
        .collect()
}

#[test]
fn calls_are_categorized() {
    let g = graph();
    let save = calls_of(&g, "save");
    let has = |t: &str, cat: &str| save.iter().any(|(a, b, _)| a == t && b == cat);
    assert!(has("format!", "macro_use"));
    assert!(has("Err", "native_rust"));
    assert!(has("println!", "macro_use"));
    assert!(has("helper", "internal_helper"), "{save:?}");
    assert!(has("check", "internal_helper"));
    // `repo: &Repo` → a fogadó típusa ismert: `Repo::store` (a `Repo` a crate `db` moduljából)
    assert!(save.contains(&(
        "store".into(),
        "module_use".into(),
        Some("Repo::store".into())
    )));
    assert!(has("to_string", "native_rust"));
    let new = calls_of(&g, "new");
    assert!(new.contains(&(
        "check".into(),
        "internal_helper".into(),
        Some("User::check".into())
    )));
    let check = calls_of(&g, "check");
    assert!(check.contains(&(
        "open_db".into(),
        "module_use".into(),
        Some("db::connect".into())
    )));
    let helper = calls_of(&g, "helper");
    assert!(helper.contains(&("cb".into(), "external_call".into(), None)));
    assert!(helper.contains(&(
        "Json".into(),
        "framework_use".into(),
        Some("axum::Json".into())
    )));
    assert!(helper.contains(&(
        "HashMap::new".into(),
        "native_rust".into(),
        Some("std::collections::HashMap::new".into())
    )));
    let works = calls_of(&g, "works");
    assert!(works.contains(&(
        "helper".into(),
        "internal_helper".into(),
        Some("users::helper".into())
    )));
}

#[test]
fn control_flow_strings_and_comments() {
    let g = graph();
    let kinds: Vec<(&str, &str, usize)> = g
        .control_flow
        .iter()
        .map(|c| {
            (
                c.kind.as_str(),
                c.caller.as_deref().unwrap(),
                c.branches.len(),
            )
        })
        .collect();
    assert_eq!(kinds, [("if", "save", 3), ("match", "check", 3)]);
    let labels: Vec<&str> = g.control_flow[0]
        .branches
        .iter()
        .map(|b| b.label.as_str())
        .collect();
    assert_eq!(
        labels,
        ["if (self.id == 0)", "elseif (self.id > 10)", "else"]
    );
    assert_eq!(g.control_flow[1].branches[1].label, "n if n > 5");
    let strings: Vec<&str> = g.strings.iter().map(|s| s.value.as_str()).collect();
    assert!(strings.contains(&"bad id {}"), "{strings:?}");
    assert_eq!(g.comments.len(), 1);
    assert_eq!(g.comments[0].tag.as_deref(), Some("TODO"));
    assert_eq!(g.comments[0].caller.as_deref(), Some("save"));
    assert_eq!(g.comments[0].container.as_deref(), Some("User"));
}
