//! A jegyzet-jelölő (`  📝 N`) a találat-sorok végén: `--grep` (minden `--depth` szinten), `locate`,
//! `--format api`. Lekérdezésenként egyszer betöltve; jegyzetfájl nélkül üres (a kimenet byte-ra azonos).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::note::Note;
use super::store::NoteStore;
use super::view::normalize;

/// A jelölő előtagja (a sor végén, két szóköz után).
const PREFIX: &str = "  📝 ";

/// `  📝 N`, vagy üres, ha `n == 0`.
pub fn mark(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("{PREFIX}{n}")
    }
}

/// A sor a jelölő nélkül (a kimenetet olvasó szabályokhoz: `FILE  <út>  📝 2` → `FILE  <út>`).
pub fn strip_mark(line: &str) -> &str {
    line.split_once(PREFIX).map_or(line, |(head, _)| head)
}

/// Fájl-jegyzetek száma (horgony-metódus nélküliek).
pub fn file_count(notes: &[Note]) -> usize {
    notes.iter().filter(|n| n.method.is_none()).count()
}

/// Egy metódus jegyzeteinek száma.
pub fn method_count(notes: &[Note], container: Option<&str>, name: &str) -> usize {
    notes
        .iter()
        .filter(|n| n.is_for_method(container, name))
        .count()
}

/// Egy fájl nem árva jegyzetei: a fájl-jegyzetek és a `methods`-ban létező metódusok jegyzetei.
pub fn live_count<'m>(
    notes: &[Note],
    methods: impl Iterator<Item = (Option<&'m str>, &'m str)> + Clone,
) -> usize {
    notes
        .iter()
        .filter(|n| {
            n.method.is_none()
                || methods
                    .clone()
                    .any(|(container, name)| n.is_for_method(container, name))
        })
        .count()
}

/// A projekt jegyzetei fájlonként (a gyökérhez relatív út szerint), egy lekérdezés idejére.
#[derive(Debug, Default)]
pub struct NoteMarks {
    root: PathBuf,
    by_file: HashMap<String, Vec<Note>>,
}

impl NoteMarks {
    /// Betöltés; nincs (vagy hibás) jegyzetfájl → üres.
    pub fn load(root: &Path) -> Self {
        let mut by_file: HashMap<String, Vec<Note>> = HashMap::new();
        for note in NoteStore::at_root(root).list().unwrap_or_default() {
            by_file.entry(note.path.clone()).or_default().push(note);
        }
        Self {
            root: root.to_path_buf(),
            by_file,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.by_file.is_empty()
    }

    /// Egy (abszolút vagy gyökérhez relatív) fájl jegyzetei.
    pub fn of(&self, file: &str) -> &[Note] {
        if self.by_file.is_empty() {
            return &[];
        }
        let path = Path::new(file);
        normalize(path.strip_prefix(&self.root).unwrap_or(path))
            .and_then(|rel| self.by_file.get(&rel))
            .map_or(&[], Vec::as_slice)
    }

    /// Egy hívó-sor jelölője: a metódus jegyzetei, ha vannak, különben a fájl-jegyzetek (`caller`
    /// `None` / `__top__` → csak a fájl).
    pub fn caller_mark(&self, file: &str, container: Option<&str>, caller: Option<&str>) -> String {
        let notes = self.of(file);
        if notes.is_empty() {
            return String::new();
        }
        let method = caller
            .filter(|name| *name != "__top__")
            .map_or(0, |name| method_count(notes, container, name));
        mark(if method > 0 {
            method
        } else {
            file_count(notes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, method: Option<&str>) -> Note {
        Note {
            v: 1,
            id: "x".into(),
            path: path.into(),
            container: None,
            method: method.map(str::to_owned),
            text: "t".into(),
            created_at: "2026-11-05T20:27:00Z".into(),
        }
    }

    #[test]
    fn counts_marks_and_strip() {
        let notes = vec![
            note("a.php", None),
            note("a.php", Some("run")),
            note("a.php", Some("run")),
            note("a.php", Some("gone")),
        ];
        assert_eq!(mark(0), "");
        assert_eq!(mark(2), "  📝 2");
        assert_eq!(file_count(&notes), 1);
        assert_eq!(method_count(&notes, Some("Shop"), "run"), 2);
        let methods = [(Some("Shop"), "run")];
        assert_eq!(live_count(&notes, methods.iter().copied()), 3);
        assert_eq!(strip_mark("FILE  a.php  📝 2"), "FILE  a.php");
        assert_eq!(strip_mark("FILE  a.php"), "FILE  a.php");
        let marks = NoteMarks {
            root: PathBuf::from("/r"),
            by_file: HashMap::from([("a.php".to_owned(), notes)]),
        };
        assert_eq!(marks.caller_mark("/r/a.php", None, Some("run")), "  📝 2");
        assert_eq!(marks.caller_mark("/r/a.php", None, Some("other")), "  📝 1");
        assert_eq!(marks.caller_mark("/r/a.php", None, Some("__top__")), "  📝 1");
        assert_eq!(marks.caller_mark("/r/b.php", None, Some("run")), "");
    }
}
