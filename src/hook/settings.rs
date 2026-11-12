//! A Claude Code `settings.json` hook-bejegyzése: hozzáadás / eltávolítás / állapot, a többi beállítás és hook
//! érintetlenül (a kulcsok sorrendje is: serde_json `preserve_order`). A szerkesztés tiszta függvényekkel történik
//! (tesztelhető); a `Settings` a fájl-I/O: biztonsági mentés (`settings.json.bak`) és atomikus írás.
//!
//! A bejegyzés alakja:
//! `{"hooks": {"PostToolUse": [{"matcher": "Edit|Write|MultiEdit", "hooks": [{"type": "command",
//! "command": "\"<bináris>\" hook post-edit", "timeout": 10}]}]}}` — a saját hookot a parancs
//! `hook post-edit` része azonosítja.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::error::{CallgraphError, Result};

/// A hook eseménye.
pub(crate) const HOOK_EVENT: &str = "PostToolUse";
/// A fájlt író eszközök (a NotebookEdit kimarad: a `.ipynb` nem elemzett nyelv, a hook úgyis semmit sem tenne).
pub(crate) const HOOK_MATCHER: &str = "Edit|Write|MultiEdit";
/// A hook időkorlátja másodpercben (a parancs azonnal kilép, a frissítés leválasztva fut).
const TIMEOUT_SECS: u64 = 10;

/// Egy saját hook leírása: esemény, matcher és a parancs azonosító része.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookSpec {
    pub(crate) event: &'static str,
    pub(crate) matcher: &'static str,
    pub(crate) marker: &'static str,
}

/// A fájl-írás utáni háttér-frissítés hookja.
pub(crate) const POST_EDIT: HookSpec = HookSpec {
    event: HOOK_EVENT,
    matcher: HOOK_MATCHER,
    marker: "hook post-edit",
};

/// A session-indítási hook (ajánlott beállítások és frissítés-értesítés; csak új sessionnél).
pub(crate) const SESSION_START: HookSpec = HookSpec {
    event: "SessionStart",
    matcher: "startup",
    marker: "hook session-start",
};

/// A hook parancssora a bináris útjával: idézőjelben (szóköz az útban), Windowson `/` elválasztóval és a `\\?\`
/// előtag nélkül (a Claude Code a hookot burokban futtatja).
pub(crate) fn hook_command(exe: &Path) -> String {
    hook_command_for(exe, &POST_EDIT)
}

/// Mint a `hook_command`, a megadott hookhoz.
pub(crate) fn hook_command_for(exe: &Path, spec: &HookSpec) -> String {
    let text = exe.to_string_lossy();
    let text = crate::query::strip_verbatim(&text);
    let text = if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.to_owned()
    };
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for ch in text.chars() {
        if matches!(ch, '"' | '\\' | '$' | '`') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    format!("{quoted} {}", spec.marker)
}

/// A beállítások szövege JSON-objektumként (üres szöveg = `{}`); hibás JSON / nem objektum: használati hiba (a
/// fájlhoz nem nyúlunk).
fn parse(text: &str, file: &Path) -> Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(invalid(file, "not a JSON object", "nem JSON-objektum")),
    }
}

fn invalid(file: &Path, en: &str, hu: &str) -> CallgraphError {
    CallgraphError::Usage(crate::tf!(
        "{}: {en} — left unchanged",
        "{}: {hu} — nem módosítottam",
        file.display()
    ))
}

fn is_ours(hook: &Value, spec: &HookSpec) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(spec.marker) && command.contains("callgraph"))
}

/// Az esemény csoportjai (ha vannak); hibás szerkezet (nem objektum / tömb): hiba.
fn groups<'a>(
    map: &'a Map<String, Value>,
    file: &Path,
    spec: &HookSpec,
) -> Result<Option<&'a Vec<Value>>> {
    let Some(hooks) = map.get("hooks") else {
        return Ok(None);
    };
    let Some(hooks) = hooks.as_object() else {
        return Err(invalid(
            file,
            "\"hooks\" is not an object",
            "a \"hooks\" nem objektum",
        ));
    };
    match hooks.get(spec.event) {
        None => Ok(None),
        Some(Value::Array(groups)) => Ok(Some(groups)),
        Some(_) => Err(invalid(
            file,
            &format!("\"hooks.{}\" is not an array", spec.event),
            &format!("a \"hooks.{}\" nem tömb", spec.event),
        )),
    }
}

/// Telepítve van-e a saját hook (bármelyik `PostToolUse` csoportban).
pub(crate) fn is_installed(text: &str, file: &Path) -> Result<bool> {
    is_installed_for(text, file, &POST_EDIT)
}

/// Telepítve van-e a megadott saját hook.
pub(crate) fn is_installed_for(text: &str, file: &Path, spec: &HookSpec) -> Result<bool> {
    let map = parse(text, file)?;
    Ok(groups(&map, file, spec)?.is_some_and(|groups| {
        groups.iter().any(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|hooks| hooks.iter().any(|h| is_ours(h, spec)))
        })
    }))
}

/// A beállítások a saját hook nélkül (a kiürült csoport, `PostToolUse` és `hooks` is kikerül); `None`, ha nem
/// volt benne (nincs mit írni).
#[cfg(test)]
pub(crate) fn uninstall(text: &str, file: &Path) -> Result<Option<String>> {
    uninstall_for(text, file, &POST_EDIT)
}

/// Mint az `uninstall`, a megadott hookra.
pub(crate) fn uninstall_for(text: &str, file: &Path, spec: &HookSpec) -> Result<Option<String>> {
    let mut map = parse(text, file)?;
    if !is_installed_for(text, file, spec)? {
        return Ok(None);
    }
    strip(&mut map, true, spec);
    Ok(Some(render(&map)?))
}

/// A saját hook kivétele a térképből (a `groups` már ellenőrizte a szerkezetet); a kiürült csoport mindig kikerül,
/// a kiürült `PostToolUse` / `hooks` csak `prune` mellett (telepítéskor a helyükön maradnak).
fn strip(map: &mut Map<String, Value>, prune: bool, spec: &HookSpec) {
    let Some(Value::Object(hooks)) = map.get_mut("hooks") else {
        return;
    };
    if let Some(Value::Array(groups)) = hooks.get_mut(spec.event) {
        for group in groups.iter_mut() {
            if let Some(Value::Array(list)) = group.get_mut("hooks") {
                list.retain(|hook| !is_ours(hook, spec));
            }
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|list| !list.is_empty())
        });
        if prune && groups.is_empty() {
            hooks.shift_remove(spec.event);
        }
    }
    if prune && hooks.is_empty() {
        map.shift_remove("hooks");
    }
}

/// A beállítások a saját hookkal (`command`); `None`, ha pontosan ez már benne van (idempotens). Egy régebbi saját
/// bejegyzés (más bináris-út) helyére kerül; a többi hook és beállítás marad.
pub(crate) fn install(text: &str, file: &Path, command: &str) -> Result<Option<String>> {
    install_for(text, file, command, &POST_EDIT)
}

/// Mint az `install`, a megadott hookkal.
pub(crate) fn install_for(
    text: &str,
    file: &Path,
    command: &str,
    spec: &HookSpec,
) -> Result<Option<String>> {
    let mut map = parse(text, file)?;
    let current = groups(&map, file, spec)?.is_some_and(|groups| {
        let ours: Vec<(Option<&str>, Option<&str>)> = groups
            .iter()
            .flat_map(|group| {
                let matcher = group.get("matcher").and_then(Value::as_str);
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|hook| is_ours(hook, spec))
                    .map(move |hook| (matcher, hook.get("command").and_then(Value::as_str)))
            })
            .collect();
        ours == [(Some(spec.matcher), Some(command))]
    });
    if current {
        return Ok(None);
    }
    strip(&mut map, false, spec);
    let entry = json!({
        "matcher": spec.matcher,
        "hooks": [{ "type": "command", "command": command, "timeout": TIMEOUT_SECS }],
    });
    let hooks = map
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(hooks) = hooks {
        let groups = hooks
            .entry(spec.event)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(groups) = groups {
            groups.push(entry);
        }
    }
    Ok(Some(render(&map)?))
}

fn render(map: &Map<String, Value>) -> Result<String> {
    Ok(serde_json::to_string_pretty(map)? + "\n")
}

/// A `settings.json` helye és I/O-ja.
pub(crate) struct Settings {
    file: PathBuf,
}

impl Settings {
    pub(crate) fn new(file: PathBuf) -> Self {
        Self { file }
    }

    /// `--settings <fájl>` › `$CLAUDE_CONFIG_DIR/settings.json` › `~/.claude/settings.json`.
    pub(crate) fn locate(explicit: Option<PathBuf>) -> Self {
        let file = explicit.unwrap_or_else(|| {
            let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    let home = std::env::var_os("HOME")
                        .or_else(|| std::env::var_os("USERPROFILE"))
                        .unwrap_or_default();
                    PathBuf::from(home).join(".claude")
                });
            dir.join("settings.json")
        });
        Self::new(file)
    }

    pub(crate) fn file(&self) -> &Path {
        &self.file
    }

    /// A fájl szövege (hiányzó fájl: üres).
    pub(crate) fn read(&self) -> Result<String> {
        match fs::read_to_string(&self.file) {
            Ok(text) => Ok(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error.into()),
        }
    }

    /// A biztonsági mentés útja (`settings.json.bak`).
    pub(crate) fn backup_file(&self) -> PathBuf {
        let mut name = self.file.as_os_str().to_owned();
        name.push(".bak");
        PathBuf::from(name)
    }

    /// Írás: előbb a régi fájl másolata (`.bak`, ha volt fájl), utána atomikusan (ideiglenes fájl, átnevezés).
    /// `true`, ha készült mentés.
    pub(crate) fn write(&self, text: &str) -> Result<bool> {
        if let Some(dir) = self.file.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let backup = self.file.is_file();
        if backup {
            fs::copy(&self.file, self.backup_file())?;
        }
        let mut tmp = self.file.as_os_str().to_owned();
        tmp.push(format!(".tmp{}", std::process::id()));
        let tmp = PathBuf::from(tmp);
        if let Err(error) = fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &self.file)) {
            let _ = fs::remove_file(&tmp);
            return Err(error.into());
        }
        Ok(backup)
    }
}
