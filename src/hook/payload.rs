//! A hook bemenete: a Claude Code `PostToolUse` JSON-ja (`tool_input.file_path`, NotebookEditnél
//! `tool_input.notebook_path`; relatív útnál a `cwd` mezőhöz képest), és a fájl projekt-gyökere.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::project::STORE_DIR;

/// A szerkesztett fájl abszolút útja a hook JSON-jából; `None`, ha a JSON hibás, a mező hiányzik / nem szöveg /
/// üres, vagy relatív út mellett nincs (abszolút) `cwd`.
pub(crate) fn edited_file(json: &str) -> Option<PathBuf> {
    let value: Value = serde_json::from_str(json).ok()?;
    let input = value.get("tool_input")?;
    let path = ["file_path", "notebook_path"]
        .iter()
        .find_map(|key| input.get(*key)?.as_str())
        .filter(|path| !path.trim().is_empty())?;
    let path = PathBuf::from(path);
    if path.is_absolute() {
        return Some(path);
    }
    let cwd = PathBuf::from(value.get("cwd")?.as_str()?);
    cwd.is_absolute().then(|| cwd.join(path))
}

/// A fájl projekt-gyökere (kanonikus): felfelé haladva az első mappa, amely regisztrált alias-gyökér (`registered`,
/// kanonikus utak), vagy amelyben már van `.callgraph/` mappa. `None`: nem ismert projekt — a hook ilyenkor semmit
/// sem csinál (nem hoz létre tárolót idegen mappában).
pub(crate) fn project_root_for(file: &Path, registered: &[PathBuf]) -> Option<PathBuf> {
    let start = file
        .parent()?
        .ancestors()
        .find_map(|dir| dir.canonicalize().ok())?;
    start
        .ancestors()
        .find(|dir| registered.iter().any(|root| root == dir) || dir.join(STORE_DIR).is_dir())
        .map(Path::to_path_buf)
}
