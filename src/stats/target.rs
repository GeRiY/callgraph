//! Egy hívás célja (fájl vagy mappa) a munkamenet-jelzőkhöz (`rel` / `rep`), a nyelvhez és a bemenet-méret
//! sávjához. A kanonikus út-szövegek CSAK a memóriában élnek: tárolás előtt kulcsolt hash lesz belőlük
//! (`Target::keys`, `hmac.rs`), a napi összesítőbe és az eseménynaplóba pedig csak a nyelv rögzített neve és a
//! méret sávja kerül.

use std::path::{Path, PathBuf};

use super::buckets::InputBucket;
use super::event::{CommandKind, Event};
use super::hmac::keyed_hex;

/// Egy befejezett, mért hívás a rögzítéshez (`StatsStore::record_call`): a sávos esemény, a vége (Unix-ms), a
/// futási idő (ms) és a cél (csak a memóriában).
#[derive(Debug, Clone)]
pub(crate) struct Call {
    pub(crate) event: Event,
    pub(crate) end_ms: u64,
    pub(crate) ms: u64,
    pub(crate) target: Option<Target>,
}

/// Egy hívás célja (a nyers, kanonikus utakkal; nem tárolódik).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    /// A célfájl (mappánál `None`).
    pub(crate) file: Option<String>,
    /// A fájl mappája, vagy maga a cél-mappa.
    pub(crate) dir: String,
    /// A projekt (gyökér vagy alias).
    pub(crate) project: String,
    /// A célfájl nyelve a rögzített listából (mappánál `None`).
    pub(crate) lang: Option<&'static str>,
    /// A célfájl méretének sávja (ismeretlen méretnél / mappánál `None`).
    pub(crate) input: Option<InputBucket>,
}

/// A cél kulcsolt hash-ei (ezek kerülnek a helyi `session.json`-ba).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Keys {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) file: Option<String>,
    pub(crate) dir: String,
    pub(crate) project: String,
}

impl Target {
    /// A kulcsolt hash-ek (`HMAC-SHA256(key, út)`); a három fajta el van választva (`f:` / `d:` / `p:`), hogy egy
    /// mappa és egy ugyanolyan nevű projekt ne essen egybe.
    pub(crate) fn keys(&self, key: &[u8]) -> Keys {
        Keys {
            file: self
                .file
                .as_ref()
                .map(|f| keyed_hex(key, &format!("f:{f}"))),
            dir: keyed_hex(key, &format!("d:{}", self.dir)),
            project: keyed_hex(key, &format!("p:{}", self.project)),
        }
    }

    /// Egy létező út a fájlrendszerből: kanonikus alak, fájlnál a mérete; `project`: a projekt azonosítója
    /// (gyökér vagy alias), `None` = a legközelebbi `.git`-es ős, különben a mappa maga.
    pub(crate) fn from_path(path: &Path, project: Option<String>) -> Option<Self> {
        let path = path.canonicalize().ok()?;
        let meta = std::fs::metadata(&path).ok()?;
        let (file, dir) = if meta.is_file() {
            (Some(path.clone()), path.parent()?.to_path_buf())
        } else if meta.is_dir() {
            (None, path.clone())
        } else {
            return None;
        };
        let project = project.unwrap_or_else(|| {
            git_root(&dir)
                .unwrap_or_else(|| dir.clone())
                .display()
                .to_string()
        });
        Some(Self {
            lang: file.as_deref().map(language),
            input: file.as_ref().map(|_| InputBucket::from_bytes(meta.len())),
            file: file.map(|f| f.display().to_string()),
            dir: dir.display().to_string(),
            project,
        })
    }

    /// Fájlrendszer nélkül, a kérés szövegéből (az MCP-n, ha a gyökér nem oldható fel; a tesztekben): a projekt
    /// az alias (vagy üres), kiterjesztéses út = fájl, különben mappa; méret nincs.
    pub(crate) fn from_text(project: Option<&str>, path: &str) -> Option<Self> {
        let path = normalize(path);
        if path.is_empty() {
            return None;
        }
        let project = project.unwrap_or_default().to_owned();
        let has_extension = Path::new(&path).extension().is_some();
        let joined = |p: &str| format!("{project}\u{0}{p}");
        Some(if has_extension {
            let parent = path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
            Self {
                file: Some(joined(&path)),
                dir: joined(&parent),
                lang: Some(language(Path::new(&path))),
                input: None,
                project,
            }
        } else {
            Self {
                file: None,
                dir: joined(&path),
                lang: None,
                input: None,
                project,
            }
        })
    }
}

/// A célfájl nyelve a kiterjesztéséből (rögzített lista; minden más `other`).
pub(crate) fn language(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "php" => "php",
        "js" | "mjs" | "cjs" | "jsx" => "js",
        "ts" | "mts" | "cts" | "tsx" => "ts",
        "vue" => "vue",
        "rs" => "rs",
        "swift" => "swift",
        "py" | "pyi" => "py",
        "go" => "go",
        "md" | "markdown" => "md",
        "json" => "json",
        "html" | "htm" => "html",
        _ => "other",
    }
}

/// A rögzített nyelvnevek (a tesztek és az adatvédelmi ellenőrzés szótára).
#[cfg(test)]
pub(crate) const LANGUAGES: [&str; 12] = [
    "php", "js", "ts", "vue", "rs", "swift", "py", "go", "md", "json", "html", "other",
];

/// Egységes alak: `/` elválasztó, `./` előtag és záró `/` nélkül.
fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut path = path.as_str();
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    path.trim_end_matches('/').to_owned()
}

/// A legközelebbi ős, amelyben `.git` van (legfeljebb 40 szint).
fn git_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .take(40)
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Ennyi argumentumot vizsgál legfeljebb a CLI-cél kereséséhez (kevés fájlrendszer-hívás).
const MAX_CANDIDATES: usize = 8;

/// A CLI-hívás célja a nyers argumentumokból: az első nem kapcsoló argumentum, amely létező fájl vagy mappa
/// (`::metódus` utótag nélkül; a `--project` mappájához vagy a munkakönyvtárhoz képest). A `locate`, `next` és
/// `project` parancsnak nincs fájl-célja.
pub(crate) fn cli_target(command: CommandKind, args: &[String], cwd: &Path) -> Option<Target> {
    if matches!(
        command,
        CommandKind::Locate | CommandKind::Next | CommandKind::Projects
    ) {
        return None;
    }
    let project_arg = flag_value(args, "--project");
    let project_dir = project_arg
        .map(|p| cwd.join(p))
        .filter(|p| p.is_dir())
        .and_then(|p| p.canonicalize().ok());
    let base = project_dir.clone().unwrap_or_else(|| cwd.to_path_buf());
    let project = project_dir
        .map(|p| p.display().to_string())
        .or_else(|| project_arg.map(str::to_owned));
    // a parancs neve (`show`, `refresh`…) nem cél; a közvetlen `callgraph <fájl>` alaknál az első argumentum az
    let skip = usize::from(command != CommandKind::Other);
    let mut candidates = Vec::new();
    let mut i = skip;
    while i < args.len() && candidates.len() < MAX_CANDIDATES {
        let arg = args[i].as_str();
        i += 1;
        if matches!(arg, "--project" | "--label" | "--cache" | "--lang") {
            i += 1;
            continue;
        }
        if arg.starts_with('-') || arg.is_empty() || arg.contains('=') {
            continue;
        }
        let path = arg.split_once("::").map_or(arg, |(p, _)| p);
        if !path.is_empty() {
            candidates.push(path.to_owned());
        }
    }
    // a nem létező út kanonikus alakja sincs: `from_path` `None`
    candidates
        .iter()
        .find_map(|c| Target::from_path(&base.join(c), project.clone()))
}

/// Egy `--flag érték` vagy `--flag=érték` alakú kapcsoló értéke.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().map(String::as_str);
        }
        if let Some(value) = arg.strip_prefix(flag).and_then(|r| r.strip_prefix('=')) {
            return Some(value);
        }
    }
    None
}
