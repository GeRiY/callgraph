//! bun: a szöveges `bun.lock` (JSONC: kommentek, záró vesszők) `packages.<név>[0]` eleme `"<név>@<verzió>"`; csak a
//! legfelső szintű kulcsok (`vue`, `@vue/shared`) — a beágyazott (`vue/@vue/shared`) és a workspace-bejegyzések
//! kimaradnak. A bináris `bun.lockb` nem olvasható, nincs a regiszterben.

use jsonc_parser::{CollectOptions, ParseOptions, ast::Value, parse_to_ast};

use super::{LockSource, installed};
use crate::packages::model::Installed;

/// A `bun.lock` stratégiája.
pub(crate) struct BunLock;

impl LockSource for BunLock {
    fn file(&self) -> &'static str {
        "bun.lock"
    }

    fn read(&self, text: &str) -> Vec<Installed> {
        let Ok(ast) = parse_to_ast(text, &CollectOptions::default(), &ParseOptions::default())
        else {
            return Vec::new();
        };
        let Some(Value::Object(root)) = ast.value else {
            return Vec::new();
        };
        let Some(Value::Object(packages)) = root.get("packages").map(|p| &p.value) else {
            return Vec::new();
        };
        packages
            .properties
            .iter()
            .filter_map(|prop| {
                let key = prop.name.as_str();
                if !top_level(key) {
                    return None;
                }
                let Value::Array(entry) = &prop.value else {
                    return None;
                };
                let Some(Value::StringLit(spec)) = entry.elements.first() else {
                    return None;
                };
                let (_, version) = spec.value.rsplit_once('@')?;
                if version.is_empty() || version.starts_with("workspace:") {
                    return None;
                }
                Some(installed(key, version))
            })
            .collect()
    }
}

/// Legfelső szintű kulcs: `név` vagy `@scope/név` (a beágyazott alak `szülő/gyerek`).
fn top_level(key: &str) -> bool {
    match key.strip_prefix('@') {
        Some(scoped) => scoped.matches('/').count() == 1,
        None => !key.contains('/'),
    }
}
