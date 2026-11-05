//! A projekt-gráf tárolt formája (`.callgraph/index.jsonl`, JSON Lines): az első sor a fejléc (formátum, verzió,
//! az eszköz build-ujjlenyomata), utána fájlonként egy rekord. Leírás: `docs/PROJECT-GRAPH.md`.

use serde::{Deserialize, Serialize};

use crate::model::FileGraph;

/// A fejléc `format` mezője: ebből tudjuk, hogy nem a régi (fejléc nélküli) hívó-index.
pub(crate) const FORMAT: &str = "callgraph-project-graph";

/// A rekord-szerkezet verziója; ha változik, a régi fájl csendben újraépül.
pub(crate) const FORMAT_VERSION: u32 = 1;

/// A fájl első sora. Bármelyik mező eltérése (más formátum, verzió vagy más build) = újraépítés.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Header {
    pub format: String,
    pub version: u32,
    /// `crate::io::tool_version()`: verzió + séma + a forrás ujjlenyomata — egy új build (pl. egy nyelvi frontend
    /// változása) a régi elemzéseket nem használja újra.
    pub tool: String,
}

impl Header {
    /// A futó build fejléce.
    pub(crate) fn current() -> Self {
        Self {
            format: FORMAT.to_owned(),
            version: FORMAT_VERSION,
            tool: crate::io::tool_version(),
        }
    }
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
    pub(crate) fn of(path: &std::path::Path) -> Self {
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

/// Egy rekord eleje: a betöltéskor csak ezt értelmezzük (a gráfot csak akkor, ha egy lekérdezés kéri).
#[derive(Debug, Deserialize)]
pub(crate) struct RecordHead {
    pub path: String,
    pub mtime: u128,
    pub size: u64,
}

/// Egy rekord tartalma: a fájl gráfja és a hívás-sorai.
#[derive(Debug, Deserialize)]
pub(crate) struct RecordBody {
    #[serde(default)]
    pub snippets: Vec<(u32, String)>,
    pub graph: FileGraph,
}

/// Egy rekord kiírása (a mezők sorrendje: a fej előbb, hogy a betöltés gyorsan átléphesse a gráfot).
#[derive(Serialize)]
pub(crate) struct RecordOut<'a> {
    pub path: &'a str,
    pub mtime: u128,
    pub size: u64,
    pub snippets: &'a [(u32, String)],
    pub graph: &'a FileGraph,
}

/// Egy fájl tényei a gráfban: a nyelvi frontend `FileGraph`-ja változatlanul (így minden új mező automatikusan
/// bekerül), és a hívás-sorok tömörített forrássora (`L12 │ …` a fájl újraolvasása nélkül).
#[derive(Debug, Clone)]
pub(crate) struct FileFacts {
    pub graph: FileGraph,
    pub snippets: Vec<(u32, String)>,
}

impl From<RecordBody> for FileFacts {
    fn from(body: RecordBody) -> Self {
        Self {
            graph: body.graph,
            snippets: body.snippets,
        }
    }
}
