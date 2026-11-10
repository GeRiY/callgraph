//! A jegyzetek megjelenítése: szűrés (hatókör + minta), árva-felismerés, és a kiírt sorok
//! (`note list`, `show --notes`, valamint az outline / `--body` jegyzet-sorai).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use regex::{Regex, RegexBuilder};

use super::note::{Note, short_time, truncate};
use super::store::NoteStore;
use crate::error::{CallgraphError, Result};
use crate::i18n::Lang;
use crate::model::FileGraph;
use crate::query::{QueryEngine, QueryResult, Selector, SelectorSet};

/// Horgonyonként legfeljebb ennyi jegyzet-sor az outline-ban / `--body`-ban (a többi `… +N`).
pub const MAX_PER_ANCHOR: usize = 3;
/// A jegyzet-sor szövegének hossza az outline-ban / `--body`-ban (karakter).
pub const MAX_TEXT: usize = 100;

/// Melyik jegyzetek kellenek: hatókör (gyökérhez relatív utak, `""` = minden) és opcionális minta.
pub struct NoteFilter {
    scope: Vec<String>,
    regex: Option<Regex>,
}

impl NoteFilter {
    /// `inputs`: fájlok / mappák (a gyökérhez relatív vagy abszolút utak; üres = az egész projekt);
    /// `pattern`: regex a szövegre vagy a horgonyra (kis- és nagybetű mindegy).
    pub fn new(root: &Path, inputs: &[PathBuf], pattern: Option<&str>) -> Result<Self> {
        let scope = inputs
            .iter()
            .map(|input| relative_path(root, input).ok_or_else(|| outside_root(input)))
            .collect::<Result<Vec<_>>>()?;
        let regex = pattern
            .map(|p| {
                RegexBuilder::new(p)
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| {
                        CallgraphError::Usage(crate::tf!(
                            "invalid pattern: {e}",
                            "hibás minta: {e}"
                        ))
                    })
            })
            .transpose()?;
        Ok(Self { scope, regex })
    }

    pub fn matches(&self, note: &Note) -> bool {
        let in_scope = self.scope.is_empty()
            || self.scope.iter().any(|s| {
                s.is_empty() || note.path == *s || note.path.starts_with(&format!("{s}/"))
            });
        in_scope
            && self
                .regex
                .as_ref()
                .is_none_or(|r| r.is_match(&note.text) || r.is_match(&note.anchor()))
    }
}

fn outside_root(input: &Path) -> CallgraphError {
    CallgraphError::Usage(crate::tf!(
        "{}: outside the project root",
        "{}: a projekt-gyökéren kívül van",
        input.display()
    ))
}

/// Egy (gyökérhez relatív vagy abszolút) út a gyökérhez képest, `/` elválasztóval; `None`, ha kívül esik.
/// `root`: kanonikus (symlink-feloldott) gyökér.
pub fn relative_path(root: &Path, input: &Path) -> Option<String> {
    let joined = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    let abs = joined.canonicalize().unwrap_or(joined);
    normalize(abs.strip_prefix(root).ok()?)
}

/// A relatív út komponensei `/`-rel (`.` kimarad, `..` visszalép); gyökérből kilépő vagy abszolút út → `None`.
pub(super) fn normalize(path: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}

/// Egy fájl állapota az árva-vizsgálathoz.
enum FileState {
    Missing,
    /// Nem elemezhető (pl. nem támogatott nyelv): nem jelöljük árvának.
    Unknown,
    Methods(Vec<(Option<String>, String)>),
}

/// Jegyzetenként: árva-e (a fájl már nincs meg, vagy a horgony-metódus nincs a fájlban). Csak a jegyzetes
/// fájlokat elemzi (cache-ből, ha friss).
pub fn orphaned(root: &Path, notes: &[Note]) -> Vec<bool> {
    let engine = QueryEngine::new(Some(root), None);
    let mut files: HashMap<&str, FileState> = HashMap::new();
    notes
        .iter()
        .map(|note| {
            let state = files.entry(note.path.as_str()).or_insert_with(|| {
                // elemezni csak akkor kell, ha a fájlnak van metódus-jegyzete
                let analyze = notes
                    .iter()
                    .any(|n| n.path == note.path && n.method.is_some());
                file_state(&engine, root, &note.path, analyze)
            });
            match (state, note.method.as_deref()) {
                (FileState::Missing, _) => true,
                (_, None) | (FileState::Unknown, _) => false,
                (FileState::Methods(methods), Some(_)) => !methods
                    .iter()
                    .any(|(c, m)| note.is_for_method(c.as_deref(), m)),
            }
        })
        .collect()
}

fn file_state(engine: &QueryEngine, root: &Path, path: &str, analyze: bool) -> FileState {
    let abs = root.join(path);
    if !abs.exists() {
        return FileState::Missing;
    }
    if !analyze || !abs.is_file() {
        return FileState::Unknown;
    }
    let selectors = SelectorSet {
        items: vec![Selector {
            file: PathBuf::from(path),
            method: None,
        }],
        selected: false,
    };
    match engine.query(&selectors) {
        Ok(rows) => FileState::Methods(
            rows.into_iter()
                .flat_map(|row| row.graph.methods)
                .map(|m| (m.container, m.name))
                .collect(),
        ),
        Err(_) => FileState::Unknown,
    }
}

/// Egy lista-sor. `root` nélkül (`note list`): `<id>  <horgony>  <idő>  📝 <szöveg>`; `root`-tal (`show --notes`):
/// `<abszolút horgony>  <idő>  📝 <szöveg>` (a gyökeret a `QueryEngine::relativize` emeli ki). Árvánál `[orphaned]`.
pub fn list_line(note: &Note, root: Option<&Path>, orphan: bool, lang: Lang) -> String {
    let mark = if orphan {
        lang.pick(" [orphaned]", " [árva]")
    } else {
        ""
    };
    let time = short_time(&note.created_at);
    match root {
        None => format!(
            "{}  {}  {time}  📝 {}{mark}",
            note.id,
            note.anchor(),
            note.text
        ),
        Some(root) => {
            let path = root.join(&note.path).display().to_string();
            let anchor = note.anchor();
            let rest = &anchor[note.path.len()..];
            format!("{path}{rest}  {time}  📝 {}{mark}", note.text)
        }
    }
}

/// A `show --notes` kimenete: a hatókör és a minta szerinti jegyzetek (árva-jelöléssel), vagy egy „nincs” sor.
pub fn search_text(root: &Path, filter: &NoteFilter, lang: Lang) -> Result<String> {
    let notes: Vec<Note> = NoteStore::at_root(root)
        .list()?
        .into_iter()
        .filter(|n| filter.matches(n))
        .collect();
    if notes.is_empty() {
        return Ok(lang
            .pick("(no matching notes)\n", "(nincs illeszkedő jegyzet)\n")
            .to_owned());
    }
    let orphans = orphaned(root, &notes);
    let mut out = String::new();
    for (note, orphan) in notes.iter().zip(orphans) {
        writeln!(out, "{}", list_line(note, Some(root), orphan, lang)).unwrap();
    }
    Ok(out)
}

/// A sorok fájljaihoz tartozó jegyzeteket a `QueryResult::notes`-ba teszi (outline / `--body` előtt).
/// Nincs jegyzetfájl vagy hibás → semmi nem változik (a kimenet byte-ra azonos marad).
pub fn annotate(rows: &mut [QueryResult], root: &Path) {
    let Ok(notes) = NoteStore::at_root(root).list() else {
        return;
    };
    if notes.is_empty() {
        return;
    }
    for row in rows {
        let Some(rel) = Path::new(&row.graph.file)
            .strip_prefix(root)
            .ok()
            .and_then(normalize)
        else {
            continue;
        };
        row.notes = notes.iter().filter(|n| n.path == rel).cloned().collect();
    }
}

/// A kiválasztott jegyzetek sorai: legújabb elöl, legfeljebb `MAX_PER_ANCHOR`, utána `… +N`.
fn push_lines(out: &mut String, indent: &str, mut picked: Vec<(&Note, String)>) {
    // azonos időnél a később felvett (a fájlban hátrébb álló) elöl
    picked.reverse();
    picked.sort_by(|a, b| b.0.created_at.cmp(&a.0.created_at));
    for (note, label) in picked.iter().take(MAX_PER_ANCHOR) {
        writeln!(
            out,
            "{indent}📝 {} · {label}{}",
            short_time(&note.created_at),
            truncate(&note.text, MAX_TEXT)
        )
        .unwrap();
    }
    if picked.len() > MAX_PER_ANCHOR {
        writeln!(out, "{indent}📝 … +{}", picked.len() - MAX_PER_ANCHOR).unwrap();
    }
}

/// Fájl-jegyzetek a `FILE` fejléc alatt (két szóköz behúzással).
pub fn file_lines(out: &mut String, notes: &[Note]) {
    let picked = notes
        .iter()
        .filter(|n| n.method.is_none())
        .map(|n| (n, String::new()))
        .collect();
    push_lines(out, "  ", picked);
}

/// Egy metódus jegyzetei (`indent` behúzással) — az outline-ban a metódus-sor alatt.
pub fn method_lines(
    out: &mut String,
    notes: &[Note],
    container: Option<&str>,
    name: &str,
    indent: &str,
) {
    let picked = notes
        .iter()
        .filter(|n| n.is_for_method(container, name))
        .map(|n| (n, String::new()))
        .collect();
    push_lines(out, indent, picked);
}

/// A `--body` jegyzet-sorai a metódus fejléce és a forrása között (behúzás nélkül).
pub fn body_lines(out: &mut String, notes: &[Note], container: Option<&str>, name: &str) {
    method_lines(out, notes, container, name, "");
}

/// Az outline-ban nem listázott függvények (procedurális fájl, vagy konténer nélküli függvény egy osztályos
/// fájlban) jegyzetei a `FILE` fejléc alatt, `név(): ` előtaggal; a fájlban nem létező (árva) horgonyúak kimaradnak.
pub fn unlisted_lines(out: &mut String, notes: &[Note], graph: &FileGraph) {
    let listed = |container: Option<&str>| {
        container.is_some_and(|c| graph.containers.iter().any(|k| k.name == c))
    };
    let picked = notes
        .iter()
        .filter_map(|n| {
            let name = n.method.as_deref()?;
            graph
                .methods
                .iter()
                .filter(|m| !listed(m.container.as_deref()))
                .any(|m| n.is_for_method(m.container.as_deref(), &m.name))
                .then(|| (n, format!("{name}(): ")))
        })
        .collect();
    push_lines(out, "  ", picked);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "callgraph-notes-view-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn note(id: &str, method: Option<&str>, text: &str, at: &str) -> Note {
        Note {
            v: 1,
            id: id.into(),
            path: "src/shop.php".into(),
            container: method.map(|_| "Shop".to_owned()),
            method: method.map(str::to_owned),
            text: text.into(),
            created_at: at.into(),
        }
    }

    #[test]
    fn orphan_detection_by_method_name() {
        let root = temp_root();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/shop.php"),
            "<?php\nclass Shop\n{\n    public function run(): int\n    {\n        return 1;\n    }\n}\n",
        )
        .unwrap();
        let notes = vec![
            note("a", None, "file", "2026-11-05T20:27:00Z"),
            note("b", Some("run"), "ok", "2026-11-05T20:27:00Z"),
            note("c", Some("gone"), "lost", "2026-11-05T20:27:00Z"),
            Note {
                path: "src/missing.php".into(),
                ..note("d", None, "missing file", "2026-11-05T20:27:00Z")
            },
        ];
        assert_eq!(orphaned(&root, &notes), vec![false, false, true, true]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn filter_scope_and_pattern() {
        let root = temp_root();
        let n = note(
            "a",
            Some("run"),
            "Cache invalidation",
            "2026-11-05T20:27:00Z",
        );
        let all = NoteFilter::new(&root, &[], None).unwrap();
        assert!(all.matches(&n));
        let dir = NoteFilter::new(&root, &[PathBuf::from("./src/")], Some("cache")).unwrap();
        assert!(dir.matches(&n));
        let by_anchor = NoteFilter::new(&root, &[PathBuf::from(".")], Some("Shop::run")).unwrap();
        assert!(by_anchor.matches(&n));
        let other = NoteFilter::new(&root, &[PathBuf::from("sr")], None).unwrap();
        assert!(!other.matches(&n));
        assert!(NoteFilter::new(&root, &[PathBuf::from("../x")], None).is_err());
        assert!(NoteFilter::new(&root, &[], Some("(")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lines_newest_first_capped_with_time() {
        let mut out = String::new();
        let notes: Vec<Note> = (1..=5)
            .map(|i| {
                note(
                    &format!("n{i}"),
                    Some("run"),
                    &format!("text {i}"),
                    &format!("2026-11-05T20:2{i}:00Z"),
                )
            })
            .collect();
        method_lines(&mut out, &notes, Some("Shop"), "run", "    ");
        assert_eq!(
            out,
            "    📝 2026-11-05 20:25 · text 5\n    📝 2026-11-05 20:24 · text 4\n    📝 2026-11-05 20:23 · text 3\n    📝 … +2\n"
        );
        let mut none = String::new();
        method_lines(&mut none, &notes, Some("Shop"), "other", "    ");
        file_lines(&mut none, &notes);
        assert!(none.is_empty());
        let long = note("x", None, &"a".repeat(150), "2026-11-05T20:27:00Z");
        let mut out = String::new();
        file_lines(&mut out, std::slice::from_ref(&long));
        assert_eq!(
            out,
            format!("  📝 2026-11-05 20:27 · {}…\n", "a".repeat(100))
        );
    }

    #[test]
    fn list_line_shapes() {
        let n = note("ab12cd34", Some("run"), "hello", "2026-11-05T20:27:13Z");
        assert_eq!(
            list_line(&n, None, false, Lang::En),
            "ab12cd34  src/shop.php::Shop::run  2026-11-05 20:27  📝 hello"
        );
        assert_eq!(
            list_line(&n, None, true, Lang::Hu),
            "ab12cd34  src/shop.php::Shop::run  2026-11-05 20:27  📝 hello [árva]"
        );
        let root = Path::new("/r");
        let shown = list_line(&n, Some(root), true, Lang::En);
        assert!(shown.ends_with("::Shop::run  2026-11-05 20:27  📝 hello [orphaned]"));
        assert!(shown.starts_with(&root.join("src/shop.php").display().to_string()));
    }

    #[test]
    fn relative_path_normalizes() {
        let root = temp_root();
        assert_eq!(relative_path(&root, Path::new(".")).as_deref(), Some(""));
        assert_eq!(
            relative_path(&root, Path::new("./a/../b/c.rs")).as_deref(),
            Some("b/c.rs")
        );
        assert_eq!(
            relative_path(&root, &root.join("x/y.rs")).as_deref(),
            Some("x/y.rs")
        );
        assert_eq!(relative_path(&root, Path::new("../outside")), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
