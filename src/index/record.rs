//! Az index egy fájljához tartozó, JSONL-ben tárolt rekordok.

use serde::{Deserialize, Serialize};

use crate::model::FileGraph;

/// Egy névfeloldott szimbólum-definíció helye.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Definition {
    pub key: String,
    pub kind: String,
    pub file: String,
    pub line: u32,
}

/// Egy már elemzett fájl indexelt állapota.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IndexRecord {
    pub path: String,
    pub mtime: u128,
    #[serde(default)]
    pub definitions: Vec<Definition>,
    pub graph: FileGraph,
}

impl IndexRecord {
    pub fn new(path: String, mtime: u128, graph: FileGraph) -> Self {
        let namespace = graph.namespace.as_deref().unwrap_or("");
        let prefix = if namespace.is_empty() {
            String::new()
        } else {
            format!("{namespace}\\")
        };
        let mut definitions = graph
            .containers
            .iter()
            .map(|container| Definition {
                key: format!("{}{name}", prefix, name = container.name),
                kind: container.kind.clone(),
                file: path.clone(),
                line: container.line,
            })
            .collect::<Vec<_>>();
        definitions.extend(graph.methods.iter().map(|method| Definition {
            key: match &method.container {
                Some(container) => format!("{}{container}::{}", prefix, method.name),
                None => format!("{}{name}", prefix, name = method.name),
            },
            kind: if method.container.is_some() {
                "method".into()
            } else {
                "function".into()
            },
            file: path.clone(),
            line: method.line,
        }));
        Self {
            path,
            mtime,
            definitions,
            graph,
        }
    }
}
