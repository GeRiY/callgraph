//! Proxy: a csomagtérkép a `.callgraph/packages.bin`-ből (MessagePack + LZ4, mint a projekt-gráf), amíg minden
//! szóba jöhető manifest- és lock-fájl bélyege (mtime + méret + ctime) változatlan, és nem jelent meg új; különben
//! újraépül és kiíródik. Ha a `.callgraph/` nincs meg (vagy nem közönséges mappa), memóriában épül, cache nélkül — a
//! csomagtérkép miatt nem jön létre a tároló.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::files::{SourceFiles, Stamp};
use super::project::{self, ProjectPackages};

/// A cache fájlneve a `.callgraph/` mappában.
const CACHE_FILE: &str = "packages.bin";
/// A tárolt szerkezet verziója; ha változik, a régi cache csendben újraépül (2: auto-discovery és plugin jelző).
const FORMAT: u32 = 2;

/// A cache tárolója (DI: a tesztek memóriában tartják).
pub(crate) trait CacheStore {
    fn load(&self) -> Option<Vec<u8>>;
    /// Kiírás; a hiba nem számít (legközelebb újraépül).
    fn save(&self, bytes: &[u8]);
}

/// A `.callgraph/packages.bin`, ha a `.callgraph/` már létező, közönséges mappa.
pub(crate) struct DirCache {
    path: Option<PathBuf>,
}

impl DirCache {
    pub(crate) fn at(root: &Path) -> Self {
        let store = crate::project::ProjectStore::at_root(root);
        let dir = store.dir();
        let usable = std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.is_dir());
        Self {
            path: usable.then(|| dir.join(CACHE_FILE)),
        }
    }
}

impl CacheStore for DirCache {
    fn load(&self) -> Option<Vec<u8>> {
        std::fs::read(self.path.as_ref()?).ok()
    }

    fn save(&self, bytes: &[u8]) {
        let Some(path) = &self.path else {
            return;
        };
        // atomikus csere: ideiglenes fájl, majd átnevezés
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".tmp{}", std::process::id()));
        let tmp = PathBuf::from(tmp);
        if std::fs::write(&tmp, bytes)
            .and_then(|()| std::fs::rename(&tmp, path))
            .is_err()
        {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// A tárolt cache: a build, a forrásfájlok bélyege és a térkép.
#[derive(Serialize, Deserialize)]
struct Stored {
    format: u32,
    tool: String,
    inputs: Vec<(PathBuf, Option<Stamp>)>,
    map: ProjectPackages,
}

/// A csomagtérkép: a cache-ből, ha naprakész, különben felépítve (és kiírva).
pub(crate) fn load_or_build(
    root: &Path,
    files: &dyn SourceFiles,
    store: &dyn CacheStore,
) -> ProjectPackages {
    let dirs = project::candidate_dirs(root, files);
    let inputs: Vec<(PathBuf, Option<Stamp>)> = project::candidate_files(&dirs)
        .into_iter()
        .map(|path| {
            let stamp = files.stamp(&path);
            (path, stamp)
        })
        .collect();
    let tool = crate::io::tool_version();
    if let Some(stored) = store.load().and_then(|bytes| decode::<Stored>(&bytes))
        && stored.format == FORMAT
        && stored.tool == tool
        && stored.inputs == inputs
        && stored.map.root.as_path() == root
    {
        return stored.map;
    }
    let map = project::build(root, &dirs, files);
    if !map.parts.is_empty() {
        let stored = Stored {
            format: FORMAT,
            tool,
            inputs,
            map,
        };
        if let Some(bytes) = encode(&stored) {
            store.save(&bytes);
        }
        return stored.map;
    }
    map
}

/// MessagePack (mezőnevekkel) + LZ4.
fn encode<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    let bytes = rmp_serde::to_vec_named(value).ok()?;
    Some(lz4_flex::block::compress_prepend_size(&bytes))
}

/// Az `encode` párja; sérült / csonka bemenetre `None`.
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    let size = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    // az LZ4 legfeljebb ~255-szörös tömörítést ad: ennél nagyobb állított méret sérült fejet jelent
    if size > bytes.len().saturating_mul(255).saturating_add(64) {
        return None;
    }
    let raw = lz4_flex::block::decompress_size_prepended(bytes).ok()?;
    rmp_serde::from_slice(&raw).ok()
}
