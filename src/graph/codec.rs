//! A projekt-gráf bináris tárolása: keretezett fájlok (`CGB1` + `[u32 hossz][bájtok]…`), az első keret a fejléc
//! (fajta, formátum-verzió, build-ujjlenyomat), a többi MessagePack (mezőnevekkel), LZ4-gyel tömörítve.
//!
//! A `FileGraph` JSON-alakú serde-je szándékosan veszteséges (pl. `Property::js`, `Constant::caller` / `js`, az üres
//! `Imports` objektum-jelzője kimarad) — a tároláshoz ezért egy veszteségmentes tükör-alak (`StoredGraph`) kell.
//! A levél-típusok (`Call`, `Method`, `Container` …) a saját derive-olt serde-jükkel kerülnek bele (mezőnevekkel, így
//! egy új mezőjük kódváltozás nélkül tárolódik); a `FileGraph`, `Property` és `Constant` mezőit a tükör kimerítő
//! szétbontással veszi át — egy új mezőjük fordítási hibát ad itt, nem csendes adatvesztést.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::record::{FileFacts, FileStamp};
use crate::error::Result;
use crate::model::{
    Call, Comment, Constant, Container, ControlFlow, FileGraph, Imports, Method, Property,
    StringLit, Write,
};

/// A tárolt szerkezet verziója; ha változik, a régi tároló csendben újraépül.
pub(crate) const FORMAT_VERSION: u32 = 2;

/// A keretezett fájlok eleje.
const MAGIC: &[u8; 4] = b"CGB1";

/// A fájlfajták (a fejléc `kind` mezője).
pub(crate) const MANIFEST: &str = "callgraph-manifest";
pub(crate) const SHARD: &str = "callgraph-shard";
pub(crate) const CALLERS: &str = "callgraph-callers";

/// Minden tárolt fájl első kerete. Bármelyik mező eltérése (más fajta, verzió vagy build) = a fájl érvénytelen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Header {
    pub kind: String,
    pub version: u32,
    /// `crate::io::tool_version()`: verzió + séma + a forrás ujjlenyomata — egy új build (pl. egy nyelvi frontend
    /// változása) a régi elemzéseket nem használja újra.
    pub tool: String,
}

impl Header {
    pub(crate) fn current(kind: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            version: FORMAT_VERSION,
            tool: crate::io::tool_version(),
        }
    }
}

/// A manifest egy sora: a forrásfájl útja, bélyege és a shard-fájl neve (`graph/<név>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManifestEntry {
    pub path: String,
    pub mtime: u64,
    pub size: u64,
    pub shard: String,
}

/// Egy fájl tárolt rekordja (shard, ill. a `callers.bin` egy kerete): az út és a bélyeg az ellenőrzéshez.
#[derive(Serialize, Deserialize)]
struct Record {
    path: String,
    mtime: u64,
    size: u64,
    graph: StoredGraph,
    snippets: Vec<(u32, String)>,
}

/// A shard-fájl neve az útból (FNV-1a 64, stabil): `<16 hex>.bin`.
pub(crate) fn shard_name(path: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}.bin")
}

/// MessagePack (mezőnevekkel) + LZ4.
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let bytes = rmp_serde::to_vec_named(value).map_err(std::io::Error::other)?;
    Ok(lz4_flex::block::compress_prepend_size(&bytes))
}

/// Az `encode` párja; sérült / csonka / hamis hosszú bemenetre `None` (nem pánikol, nem foglal irreálisan sokat).
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    let size = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    // az LZ4 legfeljebb ~255-szörös tömörítést ad: ennél nagyobb állított méret sérült fejet jelent
    if size > bytes.len().saturating_mul(255).saturating_add(64) {
        return None;
    }
    let raw = lz4_flex::block::decompress_size_prepended(bytes).ok()?;
    rmp_serde::from_slice(&raw).ok()
}

/// Atomikus írás: ideiglenes fájl (folyamat + számláló), majd átnevezés.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp{}-{n}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp);
    if let Err(error) = fs::write(&tmp, bytes).and_then(|()| fs::rename(&tmp, path)) {
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(())
}

/// Keretezett fájl írása: a fejléc, utána a már kódolt keretek.
fn write_frames(path: &Path, kind: &str, frames: &[&[u8]]) -> Result<()> {
    let header = rmp_serde::to_vec_named(&Header::current(kind)).map_err(std::io::Error::other)?;
    let total = frames.iter().map(|frame| frame.len() + 4).sum::<usize>() + header.len() + 8;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    for frame in std::iter::once(header.as_slice()).chain(frames.iter().copied()) {
        let len = u32::try_from(frame.len()).map_err(std::io::Error::other)?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(frame);
    }
    write_atomic(path, &out)
}

/// Keretezett fájl olvasása: a fejléc utáni keretek, ha a fájl ép és a fejléc a futó buildé; különben `None`.
fn read_frames(path: &Path, kind: &str) -> Option<Vec<Vec<u8>>> {
    let bytes = fs::read(path).ok()?;
    let mut rest = bytes.strip_prefix(MAGIC.as_slice())?;
    let mut frames = Vec::new();
    while !rest.is_empty() {
        let len = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        let frame = rest.get(4..4 + len)?;
        frames.push(frame.to_vec());
        rest = &rest[4 + len..];
    }
    if frames.is_empty() {
        return None;
    }
    let header: Header = rmp_serde::from_slice(&frames.remove(0)).ok()?;
    (header == Header::current(kind)).then_some(frames)
}

/// A manifest (`index.bin`) írása.
pub(crate) fn write_manifest(path: &Path, entries: &[ManifestEntry]) -> Result<()> {
    let body = encode(&entries)?;
    write_frames(path, MANIFEST, &[&body])
}

/// A manifest olvasása; hiányzó, sérült vagy más build által írt fájlra `None`.
pub(crate) fn read_manifest(path: &Path) -> Option<Vec<ManifestEntry>> {
    let frames = read_frames(path, MANIFEST)?;
    decode(frames.first()?)
}

/// Egy fájl rekordja kódolva (a shard egyetlen kerete, ill. a `callers.bin` egy kerete).
pub(crate) fn encode_record(path: &str, stamp: FileStamp, facts: &FileFacts) -> Result<Vec<u8>> {
    encode(&Record {
        path: path.to_owned(),
        mtime: stamp.mtime,
        size: stamp.size,
        graph: StoredGraph::from(&facts.graph),
        snippets: facts.snippets.clone(),
    })
}

/// Egy kódolt rekord: (út, bélyeg, tények); sérültre `None`.
pub(crate) fn decode_record(bytes: &[u8]) -> Option<(String, FileStamp, FileFacts)> {
    let record: Record = decode(bytes)?;
    let stamp = FileStamp {
        mtime: record.mtime,
        size: record.size,
    };
    let facts = FileFacts {
        graph: FileGraph::from(record.graph),
        snippets: record.snippets,
    };
    Some((record.path, stamp, facts))
}

/// Egy fájl shardja (`graph/<név>`).
pub(crate) fn write_shard(
    path: &Path,
    key: &str,
    stamp: FileStamp,
    facts: &FileFacts,
) -> Result<()> {
    let body = encode_record(key, stamp, facts)?;
    write_frames(path, SHARD, &[&body])
}

/// A shard tényei, ha a fájl ép, a futó buildé, és ugyanarról az útról, ugyanazzal a bélyeggel szól.
pub(crate) fn read_shard(path: &Path, key: &str, stamp: FileStamp) -> Option<FileFacts> {
    let frames = read_frames(path, SHARD)?;
    let (path, stored, facts) = decode_record(frames.first()?)?;
    (path == key && stored == stamp).then_some(facts)
}

/// A `callers.bin` írása: fájlonként egy (már kódolt) rekord-keret.
pub(crate) fn write_callers(path: &Path, frames: &[&[u8]]) -> Result<()> {
    write_frames(path, CALLERS, frames)
}

/// A `callers.bin` keretei (a dekódolás a hívónál, párhuzamosan); érvénytelen fájlra üres.
pub(crate) fn read_callers(path: &Path) -> Vec<Vec<u8>> {
    read_frames(path, CALLERS).unwrap_or_default()
}

/// A `FileGraph` veszteségmentes tároló-alakja.
#[derive(Serialize, Deserialize)]
struct StoredGraph {
    file: String,
    module: Option<String>,
    framework: Option<String>,
    namespace: Option<String>,
    imports: Vec<(String, String)>,
    /// Az üres import-térkép `{}`-ként (nem `[]`-ként) íródik-e ki.
    imports_object: bool,
    containers: Vec<Container>,
    properties: Vec<StoredProperty>,
    constants: Vec<StoredConstant>,
    methods: Vec<Method>,
    calls: Vec<Call>,
    comments: Vec<Comment>,
    control_flow: Vec<ControlFlow>,
    strings: Vec<StringLit>,
    writes: Vec<Write>,
    /// A `.vue` SFC-leírás JSON-szövegként (a `serde_json::Value` így pontosan visszaáll).
    sfc: Option<String>,
}

impl From<&FileGraph> for StoredGraph {
    fn from(graph: &FileGraph) -> Self {
        // kimerítő szétbontás: egy új FileGraph-mező itt fordítási hibát ad
        let FileGraph {
            file,
            module,
            framework,
            namespace,
            imports,
            containers,
            properties,
            constants,
            methods,
            calls,
            comments,
            control_flow,
            strings,
            writes,
            sfc,
        } = graph;
        let (entries, object) = imports.parts();
        Self {
            file: file.clone(),
            module: module.clone(),
            framework: framework.clone(),
            namespace: namespace.clone(),
            imports: entries.to_vec(),
            imports_object: object,
            containers: containers.clone(),
            properties: properties.iter().map(StoredProperty::from).collect(),
            constants: constants.iter().map(StoredConstant::from).collect(),
            methods: methods.clone(),
            calls: calls.clone(),
            comments: comments.clone(),
            control_flow: control_flow.clone(),
            strings: strings.clone(),
            writes: writes.clone(),
            sfc: sfc.as_ref().map(serde_json::Value::to_string),
        }
    }
}

impl From<StoredGraph> for FileGraph {
    fn from(stored: StoredGraph) -> Self {
        Self {
            file: stored.file,
            module: stored.module,
            framework: stored.framework,
            namespace: stored.namespace,
            imports: Imports::from_parts(stored.imports, stored.imports_object),
            containers: stored.containers,
            properties: stored.properties.into_iter().map(Property::from).collect(),
            constants: stored.constants.into_iter().map(Constant::from).collect(),
            methods: stored.methods,
            calls: stored.calls,
            comments: stored.comments,
            control_flow: stored.control_flow,
            strings: stored.strings,
            writes: stored.writes,
            sfc: stored.sfc.and_then(|text| serde_json::from_str(&text).ok()),
        }
    }
}

/// A `Property` minden mezője (a JSON-alakból kimaradó `js` is).
#[derive(Serialize, Deserialize)]
struct StoredProperty {
    container: Option<String>,
    name: String,
    line: u32,
    type_hint: Option<String>,
    visibility: String,
    is_static: bool,
    readonly: bool,
    doc: Option<String>,
    js: bool,
}

impl From<&Property> for StoredProperty {
    fn from(property: &Property) -> Self {
        let Property {
            container,
            name,
            line,
            type_hint,
            visibility,
            is_static,
            readonly,
            doc,
            js,
        } = property;
        Self {
            container: container.clone(),
            name: name.clone(),
            line: *line,
            type_hint: type_hint.clone(),
            visibility: visibility.clone(),
            is_static: *is_static,
            readonly: *readonly,
            doc: doc.clone(),
            js: *js,
        }
    }
}

impl From<StoredProperty> for Property {
    fn from(stored: StoredProperty) -> Self {
        Self {
            container: stored.container,
            name: stored.name,
            line: stored.line,
            type_hint: stored.type_hint,
            visibility: stored.visibility,
            is_static: stored.is_static,
            readonly: stored.readonly,
            doc: stored.doc,
            js: stored.js,
        }
    }
}

/// A `Constant` minden mezője (a JSON-alakból kimaradó `caller` és `js` is).
#[derive(Serialize, Deserialize)]
struct StoredConstant {
    container: Option<String>,
    kind: String,
    name: String,
    line: u32,
    value: Option<String>,
    visibility: Option<String>,
    caller: Option<String>,
    doc: Option<String>,
    js: bool,
}

impl From<&Constant> for StoredConstant {
    fn from(constant: &Constant) -> Self {
        let Constant {
            container,
            kind,
            name,
            line,
            value,
            visibility,
            caller,
            doc,
            js,
        } = constant;
        Self {
            container: container.clone(),
            kind: kind.clone(),
            name: name.clone(),
            line: *line,
            value: value.clone(),
            visibility: visibility.clone(),
            caller: caller.clone(),
            doc: doc.clone(),
            js: *js,
        }
    }
}

impl From<StoredConstant> for Constant {
    fn from(stored: StoredConstant) -> Self {
        Self {
            container: stored.container,
            kind: stored.kind,
            name: stored.name,
            line: stored.line,
            value: stored.value,
            visibility: stored.visibility,
            caller: stored.caller,
            doc: stored.doc,
            js: stored.js,
        }
    }
}
