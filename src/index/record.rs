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

/// Egy fájl változás-bélyege: mtime (ns) + méret. A méret azért kell, mert több fájlrendszer
/// (pl. Docker bind mount) durva, ms-os mtime-ot ad: két gyors írás azonos mtime-ot kaphat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileStamp {
    pub mtime: u128,
    pub size: u64,
}

impl FileStamp {
    /// A fájl metaadataiból; ha nem olvasható, az alapérték (mindig eltér a tárolttól).
    pub fn of(path: &std::path::Path) -> Self {
        let Ok(meta) = path.metadata() else {
            return Self::default();
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        Self {
            mtime,
            size: meta.len(),
        }
    }
}

/// Egy már elemzett fájl indexelt állapota.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IndexRecord {
    pub path: String,
    pub mtime: u128,
    /// A régi (méret nélküli) rekordoknál 0 → a fájl egyszer újraelemződik.
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub definitions: Vec<Definition>,
    pub graph: FileGraph,
}

impl IndexRecord {
    pub fn stamp(&self) -> FileStamp {
        FileStamp {
            mtime: self.mtime,
            size: self.size,
        }
    }

    pub fn new(path: String, stamp: FileStamp, graph: FileGraph) -> Self {
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
            mtime: stamp.mtime,
            size: stamp.size,
            definitions,
            graph,
        }
    }
}
