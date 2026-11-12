//! A projekt-gráf fájlonkénti adatai: a változás-bélyeg és a tények (gráf + hívás-sorok).

use crate::model::FileGraph;

/// Egy fájl változás-bélyege: mtime (ns) + méret + (unixon) ctime (ns). A méret azért kell, mert több fájlrendszer
/// (pl. Docker bind mount) durva, ms-os mtime-ot ad: két gyors írás azonos mtime-ot kaphat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FileStamp {
    pub mtime: u64,
    pub size: u64,
    /// Az inode-változás ideje (ns); unixon `touch -d`-vel / visszaállított mtime-mal sem hamisítható. Windowson 0
    /// (ott a ctime a létrehozás ideje, ami nem jelzi a változást).
    pub ctime: u64,
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
            ctime: ctime_ns(&meta),
        }
    }
}

/// Az inode-változás ideje ns-ban (unix); máshol 0.
#[cfg(unix)]
fn ctime_ns(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let secs = u64::try_from(meta.ctime()).unwrap_or(0);
    let nanos = u64::try_from(meta.ctime_nsec()).unwrap_or(0);
    secs.saturating_mul(1_000_000_000).saturating_add(nanos)
}

#[cfg(not(unix))]
fn ctime_ns(_: &std::fs::Metadata) -> u64 {
    0
}

/// Egy fájl tényei a gráfban: a nyelvi frontend `FileGraph`-ja változatlanul (így minden új levél-mező automatikusan
/// bekerül), és a hívás-sorok tömörített forrássora (`L12 │ …` a fájl újraolvasása nélkül).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileFacts {
    pub graph: FileGraph,
    pub snippets: Vec<(u32, String)>,
}

impl FileFacts {
    /// A `facts.bin` szűkített rekordja — minden, amit a teljes hatókörű lekérdezések olvasnak: a hívó-index és a
    /// típus-hierarchia (névtér, konténerek, metódusok, hívások), a `--grep` outline (ugyanezek + a vezérlési ágak; a
    /// `--grep` szűrő a property-ket, konstansokat és kommenteket úgyis eldobja), a `--strings` (string-literálok), a
    /// `--const` (konstans-definíciók és `const_fetch` hívások), a `locate` (a property-k is: definícióként
    /// kereshetők, a doc-juk nélkül), és a hívás-sorok. Kimarad: kommentek, írások, importok, SFC, a
    /// hívás-argumentumok, a property- és metódus-doc, a metódus-törzs. Ha e lekérdezések egy itt kiürített
    /// mezőt kezdenének olvasni, a gráf–bejárás összevető teszt (`tests/project_graph.rs`) eltérést jelez.
    pub(crate) fn slim(&self) -> Self {
        let source = &self.graph;
        let graph = FileGraph {
            file: source.file.clone(),
            module: source.module.clone(),
            framework: source.framework.clone(),
            namespace: source.namespace.clone(),
            imports: crate::model::Imports::default(),
            containers: source.containers.clone(),
            properties: source
                .properties
                .iter()
                .map(|property| crate::model::Property {
                    doc: None,
                    ..property.clone()
                })
                .collect(),
            constants: source.constants.clone(),
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
            control_flow: source.control_flow.clone(),
            strings: source.strings.clone(),
            writes: Vec::new(),
            sfc: None,
            binary: source.binary,
            parse_errors: source.parse_errors.clone(),
        };
        Self {
            graph,
            snippets: self.snippets.clone(),
        }
    }
}
