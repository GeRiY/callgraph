//! A projekt-gráf fájlonkénti adatai: a változás-bélyeg és a tények (gráf + hívás-sorok).

use crate::model::FileGraph;

/// Egy fájl változás-bélyege: mtime (ns) + méret. A méret azért kell, mert több fájlrendszer
/// (pl. Docker bind mount) durva, ms-os mtime-ot ad: két gyors írás azonos mtime-ot kaphat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileStamp {
    pub mtime: u64,
    pub size: u64,
}

impl FileStamp {
    /// A fájl metaadataiból (stat, a tartalmat nem olvassa); ha nem olvasható, az alapérték (mindig eltér a
    /// tárolttól).
    pub(crate) fn of(path: &std::path::Path) -> Self {
        let Ok(meta) = path.metadata() else {
            return Self::default();
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
        Self {
            mtime,
            size: meta.len(),
        }
    }
}

/// Egy fájl tényei a gráfban: a nyelvi frontend `FileGraph`-ja változatlanul (így minden új levél-mező automatikusan
/// bekerül), és a hívás-sorok tömörített forrássora (`L12 │ …` a fájl újraolvasása nélkül).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileFacts {
    pub graph: FileGraph,
    pub snippets: Vec<(u32, String)>,
}

impl FileFacts {
    /// A hívó-index (`callers.bin`) bemenete: a gráf a hívó-index, a típus-hierarchia és a metódus-tartományok
    /// mezőivel (névtér, konténerek, metódusok, hívások — a hívás-argumentumok nélkül), és a hívás-sorok. Ha a
    /// `CallerIndex` / `TypeHierarchy` egy itt kiürített mezőt kezdene olvasni, a gráf–bejárás összevető teszt
    /// (`tests/project_graph.rs`) eltérést jelez.
    pub(crate) fn slim(&self) -> Self {
        let source = &self.graph;
        let graph = FileGraph {
            file: source.file.clone(),
            module: source.module.clone(),
            framework: source.framework.clone(),
            namespace: source.namespace.clone(),
            imports: crate::model::Imports::default(),
            containers: source.containers.clone(),
            properties: Vec::new(),
            constants: Vec::new(),
            methods: source
                .methods
                .iter()
                .map(|method| crate::model::Method {
                    doc: None,
                    body: None,
                    ..method.clone()
                })
                .collect(),
            calls: source
                .calls
                .iter()
                .map(|call| crate::model::Call {
                    args: None,
                    ..call.clone()
                })
                .collect(),
            comments: Vec::new(),
            control_flow: Vec::new(),
            strings: Vec::new(),
            writes: Vec::new(),
            sfc: None,
        };
        Self {
            graph,
            snippets: self.snippets.clone(),
        }
    }
}
