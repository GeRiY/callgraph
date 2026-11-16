//! npm: a `package.json` (`dependencies`, `peerDependencies` — nem dev —, `devDependencies`; a `workspaces` tömb
//! vagy `{packages: […]}` alakban) és a `package-lock.json` (v2 / v3: `packages["node_modules/<név>"].version`, csak a
//! legfelső szint; v1: `dependencies.<név>.version`).

use serde_json::Value;

use super::{LockSource, ManifestSource, installed, push_deps, text_field};
use crate::packages::model::{Installed, Manifest};

/// A `package.json` stratégiája.
pub(crate) struct Npm;

/// A `package-lock.json` stratégiája.
pub(crate) struct NpmLock;

impl ManifestSource for Npm {
    fn file(&self) -> &'static str {
        "package.json"
    }

    fn read(&self, text: &str) -> Option<Manifest> {
        let value: Value = serde_json::from_str(text).ok()?;
        if !value.is_object() {
            return None;
        }
        let mut deps = Vec::new();
        push_deps(&mut deps, value.get("dependencies"), false, |_| true);
        push_deps(&mut deps, value.get("peerDependencies"), false, |_| true);
        push_deps(&mut deps, value.get("devDependencies"), true, |_| true);
        let workspaces: Vec<String> = value
            .get("workspaces")
            .map(|w| w.get("packages").unwrap_or(w))
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Some(Manifest {
            deps,
            own_namespaces: Vec::new(),
            name: text_field(&value, "name"),
            workspaces,
        })
    }
}

impl LockSource for NpmLock {
    fn file(&self) -> &'static str {
        "package-lock.json"
    }

    fn read(&self, text: &str) -> Vec<Installed> {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return Vec::new();
        };
        if let Some(packages) = value.get("packages").and_then(Value::as_object) {
            return packages
                .iter()
                .filter_map(|(key, entry)| {
                    let name = key.strip_prefix("node_modules/")?;
                    if name.contains("/node_modules/") {
                        return None;
                    }
                    Some(installed(name, entry.get("version")?.as_str()?))
                })
                .collect();
        }
        value
            .get("dependencies")
            .and_then(Value::as_object)
            .map(|deps| {
                deps.iter()
                    .filter_map(|(name, entry)| {
                        Some(installed(name, entry.get("version")?.as_str()?))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
