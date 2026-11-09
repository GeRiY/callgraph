//! A projekt-gráf bináris tárolása: keretezett fájlok (`CGB1` + `[u32 hossz][bájtok]…`), az első keret a fejléc
//! (fajta, formátum-verzió, build-ujjlenyomat), a többi MessagePack (mezőnevekkel), LZ4-gyel tömörítve. A pack
//! (`graph.pack`) csak hozzáfűz; a rekordjai helyét a manifest (`index.bin`) tartja.
//!
//! A `FileGraph` JSON-alakú serde-je szándékosan veszteséges (pl. `Property::js`, `Constant::caller` / `js`, az üres
//! `Imports` objektum-jelzője kimarad) — a tároláshoz ezért egy veszteségmentes tükör-alak (`StoredGraph`) kell.
//! A levél-típusok (`Call`, `Method`, `Container` …) a saját derive-olt serde-jükkel kerülnek bele (mezőnevekkel, így
//! egy új mezőjük kódváltozás nélkül tárolódik); a `FileGraph`, `Property` és `Constant` mezőit a tükör kimerítő
//! szétbontással veszi át — egy új mezőjük fordítási hibát ad itt, nem csendes adatvesztést.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::record::{FileFacts, FileStamp};
use crate::error::Result;
use crate::model::{
    Call, Comment, Constant, Container, ControlFlow, FileGraph, Imports, Method, ParseError,
    Property, StringLit, Write,
};

/// A tárolt szerkezet verziója; ha változik, a régi tároló csendben újraépül.
pub(crate) const FORMAT_VERSION: u32 = 3;

/// A keretezett fájlok eleje.
const MAGIC: &[u8; 4] = b"CGB1";

/// A fájlfajták (a fejléc `kind` mezője).
pub(crate) const MANIFEST: &str = "callgraph-manifest";
pub(crate) const PACK: &str = "callgraph-pack";
pub(crate) const FACTS: &str = "callgraph-facts";

/// Ennél több kért rekordnál a pack egyetlen olvasással jön be (különben rekordonként pozícionált olvasás).
const WHOLE_PACK_READ: usize = 16;

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

/// A manifest egy sora: a forrásfájl útja, bélyege, és a teljes rekordja helye a packben (a keret tartalmának
/// eleje és hossza).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManifestEntry {
    pub path: String,
    pub mtime: u64,
    pub size: u64,
    pub offset: u64,
    pub len: u32,
}

/// Egy fájl tárolt rekordja (a pack, ill. a `facts.bin` egy kerete): az út és a bélyeg az ellenőrzéshez.
#[derive(Serialize, Deserialize)]
struct Record {
    path: String,
    mtime: u64,
    size: u64,
    graph: StoredGraph,
    snippets: Vec<(u32, String)>,
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

/// Egy keret hozzáfűzése a pufferhez; a visszatérési érték a keret tartalmának helye (eltolás, hossz).
fn push_frame(out: &mut Vec<u8>, frame: &[u8]) -> Result<(u64, u32)> {
    let len = u32::try_from(frame.len()).map_err(std::io::Error::other)?;
    out.extend_from_slice(&len.to_le_bytes());
    let offset = out.len() as u64;
    out.extend_from_slice(frame);
    Ok((offset, len))
}

/// A fájl eleje: `CGB1` + a fejléc-keret.
fn file_head(kind: &str) -> Result<Vec<u8>> {
    let header = rmp_serde::to_vec_named(&Header::current(kind)).map_err(std::io::Error::other)?;
    let mut out = Vec::with_capacity(header.len() + 8);
    out.extend_from_slice(MAGIC);
    push_frame(&mut out, &header)?;
    Ok(out)
}

/// Keretezett fájl írása (atomikusan): a fejléc, utána a már kódolt keretek; a keretek helye a fájlban.
fn write_frames(path: &Path, kind: &str, frames: &[&[u8]]) -> Result<Vec<(u64, u32)>> {
    let mut out = file_head(kind)?;
    out.reserve(frames.iter().map(|frame| frame.len() + 4).sum::<usize>());
    let places = frames
        .iter()
        .map(|frame| push_frame(&mut out, frame))
        .collect::<Result<Vec<_>>>()?;
    write_atomic(path, &out)?;
    Ok(places)
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
    write_frames(path, MANIFEST, &[&body]).map(|_| ())
}

/// A manifest olvasása; hiányzó, sérült vagy más build által írt fájlra `None`.
pub(crate) fn read_manifest(path: &Path) -> Option<Vec<ManifestEntry>> {
    let frames = read_frames(path, MANIFEST)?;
    decode(frames.first()?)
}

/// Egy fájl rekordja kódolva (a pack, ill. a `facts.bin` egy kerete).
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

/// Új pack írása (atomikusan) a megadott rekord-keretekkel; a keretek helye.
pub(crate) fn write_pack(path: &Path, frames: &[&[u8]]) -> Result<Vec<(u64, u32)>> {
    write_frames(path, PACK, frames)
}

/// Rekord-keretek hozzáfűzése a meglévő packhez (egyetlen írással); ha nincs pack, újat ír. A félbeszakadt
/// hozzáfűzés csak holt terület: a manifest csak utána íródik, és minden rekord a saját útját és bélyegét is hordozza.
pub(crate) fn append_pack(path: &Path, frames: &[&[u8]]) -> Result<Vec<(u64, u32)>> {
    let len = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if len == 0 {
        return write_pack(path, frames);
    }
    let mut out = Vec::with_capacity(frames.iter().map(|frame| frame.len() + 4).sum());
    let places = frames
        .iter()
        .map(|frame| push_frame(&mut out, frame).map(|(offset, size)| (offset + len, size)))
        .collect::<Result<Vec<_>>>()?;
    let mut file = fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(&out)?;
    file.flush()?;
    Ok(places)
}

/// A pack fájl mérete (a holt terület becsléséhez).
pub(crate) fn pack_len(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// A kért rekord-keretek a packből, egyetlen megnyitással: kevés kérésnél pozícionált olvasással, sok kérésnél a
/// fájl egyetlen olvasásával. Hiányzó / csonka helyre `None` (a hívó újraelemez).
pub(crate) fn read_pack(path: &Path, places: &[(u64, u32)]) -> Vec<Option<Vec<u8>>> {
    let slice = |bytes: &[u8], (offset, len): (u64, u32)| {
        let start = usize::try_from(offset).ok()?;
        bytes
            .get(start..start.checked_add(len as usize)?)
            .map(<[u8]>::to_vec)
    };
    if places.len() > WHOLE_PACK_READ {
        return match fs::read(path) {
            Ok(bytes) => places
                .iter()
                .map(|place| slice(bytes.as_slice(), *place))
                .collect(),
            Err(_) => vec![None; places.len()],
        };
    }
    let Ok(mut file) = fs::File::open(path) else {
        return vec![None; places.len()];
    };
    // eltolás szerint rendezve olvasunk (előre haladó olvasás), az eredmény a kérés sorrendjében
    let mut order: Vec<usize> = (0..places.len()).collect();
    order.sort_by_key(|at| places[*at].0);
    let mut out = vec![None; places.len()];
    for at in order {
        let (offset, len) = places[at];
        let mut buf = vec![0; len as usize];
        if file.seek(SeekFrom::Start(offset)).is_ok() && file.read_exact(&mut buf).is_ok() {
            out[at] = Some(buf);
        }
    }
    out
}

/// A `facts.bin` írása: fájlonként egy (már kódolt, szűkített) rekord-keret.
pub(crate) fn write_facts(path: &Path, frames: &[&[u8]]) -> Result<()> {
    write_frames(path, FACTS, frames).map(|_| ())
}

/// A `facts.bin` keretei (egyetlen olvasás; a dekódolás a hívónál, párhuzamosan); érvénytelen fájlra üres.
pub(crate) fn read_facts(path: &Path) -> Vec<Vec<u8>> {
    read_frames(path, FACTS).unwrap_or_default()
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
    /// Bináris tartalmú fájl (a régi rekordokban nincs: hamis).
    #[serde(default)]
    binary: bool,
    /// A szintaktikai hibák (a régi rekordokban nincs: üres).
    #[serde(default)]
    parse_errors: Vec<ParseError>,
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
            binary,
            parse_errors,
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
            binary: *binary,
            parse_errors: parse_errors.clone(),
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
            binary: stored.binary,
            parse_errors: stored.parse_errors,
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
