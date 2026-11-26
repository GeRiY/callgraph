//! `callgraph setup`: one command that wires callgraph into the user's AI coding agents. For every selected (or
//! detected) client it writes (1) the MCP server entry (`<this binary> mcp`) into the client's config file and (2) a
//! short "use callgraph first" instruction in the client's instruction channel (the adoption lever: without it the
//! model keeps reaching for Grep / Read).
//!
//! Safety: JSON / TOML are merged without dropping anything else, a changed file gets a `.bak` copy (once per run),
//! a second run changes nothing, `--dry-run` writes nothing, `--uninstall` removes only callgraph's own entry /
//! marker block / rule file (the rule files carry `OWNED_MARK`; a file without it is never touched).
//!
//! Config formats (from the clients' public docs; the unsure ones are deliberately not implemented):
//! - Claude Code: project `.mcp.json` and user `~/.claude.json` (what `claude mcp add -s user` writes), both
//!   `{"mcpServers": {"callgraph": {"command": …, "args": ["mcp"]}}}`; rule `.claude/rules/callgraph.md`.
//! - Cursor: `.cursor/mcp.json` / `~/.cursor/mcp.json`, same `mcpServers` shape; rule `.cursor/rules/callgraph.mdc`
//!   (frontmatter `alwaysApply: true`). Cursor has no file-based user-level rules.
//! - VS Code: `.vscode/mcp.json` / user `<config>/Code/User/mcp.json`, `{"servers": {"callgraph": {"type":
//!   "stdio", "command": …, "args": ["mcp"]}}}`; instruction: `.github/copilot-instructions.md` marker block.
//! - OpenAI Codex CLI: `~/.codex/config.toml` (`$CODEX_HOME`), `[mcp_servers.callgraph]` with `command` / `args`;
//!   instruction: `AGENTS.md` marker block (project root, or `~/.codex/AGENTS.md` with `--user`).
//! - Windsurf: `~/.codeium/windsurf/mcp_config.json`, `mcpServers` shape; instruction: `AGENTS.md` marker block.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::error::{CallgraphError, Result};
use crate::hook::Settings;

#[cfg(test)]
mod tests;

pub const BLOCK_START: &str = "<!-- callgraph:start -->";
pub const BLOCK_END: &str = "<!-- callgraph:end -->";
/// Marks a rule file written by `setup` (only such a file is updated / removed by it).
pub const OWNED_MARK: &str = "<!-- callgraph:setup -->";
const SERVER: &str = "callgraph";

const RULE_TEXT: &str = "Navigate code with the callgraph MCP tools, not with Grep / Read: `locate` with all names from the task, then `outline` of the top files; `method_body` only for the one or two methods you must read. Grep only for exact text; Read only for a known line range.";

/// A supported AI coding client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Client {
    Claude,
    Cursor,
    Codex,
    VsCode,
    Windsurf,
}

impl Client {
    pub const ALL: [Client; 5] = [
        Client::Claude,
        Client::Cursor,
        Client::Codex,
        Client::VsCode,
        Client::Windsurf,
    ];

    pub fn parse(name: &str) -> Option<Client> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude-code" => Client::Claude,
            "cursor" => Client::Cursor,
            "codex" => Client::Codex,
            "vscode" | "vs-code" | "code" | "copilot" => Client::VsCode,
            "windsurf" => Client::Windsurf,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Client::Claude => "Claude Code",
            Client::Cursor => "Cursor",
            Client::Codex => "Codex CLI",
            Client::VsCode => "VS Code",
            Client::Windsurf => "Windsurf",
        }
    }
}

/// The flags of `callgraph setup`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Empty: detect.
    pub clients: Vec<Client>,
    /// Project directory (default: the current directory).
    pub project: Option<PathBuf>,
    pub user: bool,
    pub dry_run: bool,
    pub uninstall: bool,
}

/// The machine facts the plan depends on (injectable for tests).
#[derive(Debug, Clone)]
pub struct Env {
    pub project: PathBuf,
    pub home: PathBuf,
    /// Platform config dir (VS Code user settings live under `<config>/Code/User`).
    pub config_dir: PathBuf,
    pub codex_home: PathBuf,
    pub exe: PathBuf,
    pub path_dirs: Vec<PathBuf>,
}

impl Env {
    pub fn from_system(project: PathBuf) -> Result<Env> {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = var("HOME")
            .or_else(|| var("USERPROFILE"))
            .unwrap_or_default();
        let config_dir = if cfg!(windows) {
            var("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming"))
        } else if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            var("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"))
        };
        let exe = std::env::current_exe()?;
        let exe = PathBuf::from(crate::query::strip_verbatim(&exe.to_string_lossy()));
        let path_dirs = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        Ok(Env {
            codex_home: var("CODEX_HOME").unwrap_or_else(|| home.join(".codex")),
            project,
            home,
            config_dir,
            exe,
            path_dirs,
        })
    }

    fn on_path(&self, name: &str) -> bool {
        self.path_dirs.iter().any(|dir| {
            ["", ".exe", ".cmd"]
                .iter()
                .any(|ext| dir.join(format!("{name}{ext}")).is_file())
        })
    }
}

/// Cheap presence check: the client's CLI is on PATH, or its config dir exists (user or project level).
pub fn detect(client: Client, env: &Env) -> bool {
    let (h, p) = (&env.home, &env.project);
    match client {
        Client::Claude => {
            env.on_path("claude")
                || h.join(".claude").is_dir()
                || p.join(".claude").is_dir()
                || p.join(".mcp.json").is_file()
        }
        Client::Cursor => {
            env.on_path("cursor") || h.join(".cursor").is_dir() || p.join(".cursor").is_dir()
        }
        Client::Codex => env.on_path("codex") || env.codex_home.is_dir(),
        Client::VsCode => {
            env.on_path("code") || h.join(".vscode").is_dir() || p.join(".vscode").is_dir()
        }
        Client::Windsurf => env.on_path("windsurf") || h.join(".codeium/windsurf").is_dir(),
    }
}

enum Kind {
    Json { key: &'static str, entry: Value },
    Toml { section: String },
    Block { body: String },
    File { content: String },
}

struct Op {
    path: PathBuf,
    kind: Kind,
}

enum Step {
    Op(Op),
    Skip {
        what: &'static str,
        reason: &'static str,
    },
}

fn exe_str(env: &Env) -> String {
    env.exe.to_string_lossy().into_owned()
}

fn server_entry(env: &Env, typed: bool) -> Value {
    if typed {
        json!({"type": "stdio", "command": exe_str(env), "args": ["mcp"]})
    } else {
        json!({"command": exe_str(env), "args": ["mcp"]})
    }
}

fn md_rule() -> String {
    format!("{OWNED_MARK}\n{RULE_TEXT}\n")
}

fn cursor_rule() -> String {
    format!(
        "---\ndescription: Navigate code with callgraph first\nalwaysApply: true\n---\n{OWNED_MARK}\n{RULE_TEXT}\n"
    )
}

fn block_body() -> String {
    format!("## Code navigation (callgraph)\n{RULE_TEXT}")
}

fn toml_str(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn toml_section(env: &Env) -> String {
    format!(
        "[mcp_servers.{SERVER}]\ncommand = {}\nargs = [\"mcp\"]\n",
        toml_str(&exe_str(env))
    )
}

fn plan(client: Client, env: &Env, user: bool) -> Vec<Step> {
    let (h, p) = (&env.home, &env.project);
    let json_op = |path: PathBuf, key, typed| {
        Step::Op(Op {
            path,
            kind: Kind::Json {
                key,
                entry: server_entry(env, typed),
            },
        })
    };
    let file_op = |path: PathBuf, content: String| {
        Step::Op(Op {
            path,
            kind: Kind::File { content },
        })
    };
    let block_op = |path: PathBuf| {
        Step::Op(Op {
            path,
            kind: Kind::Block { body: block_body() },
        })
    };
    match client {
        Client::Claude => {
            let dir = if user {
                h.join(".claude")
            } else {
                p.join(".claude")
            };
            let mcp = if user {
                h.join(".claude.json")
            } else {
                p.join(".mcp.json")
            };
            vec![
                json_op(mcp, "mcpServers", false),
                file_op(dir.join("rules/callgraph.md"), md_rule()),
            ]
        }
        Client::Cursor => {
            if user {
                vec![
                    json_op(h.join(".cursor/mcp.json"), "mcpServers", false),
                    Step::Skip {
                        what: "rule",
                        reason: "Cursor has no file-based user rules (run without --user)",
                    },
                ]
            } else {
                vec![
                    json_op(p.join(".cursor/mcp.json"), "mcpServers", false),
                    file_op(p.join(".cursor/rules/callgraph.mdc"), cursor_rule()),
                ]
            }
        }
        Client::VsCode => {
            if user {
                vec![
                    json_op(env.config_dir.join("Code/User/mcp.json"), "servers", true),
                    Step::Skip {
                        what: "instruction",
                        reason: "no user-level instruction file (run without --user)",
                    },
                ]
            } else {
                vec![
                    json_op(p.join(".vscode/mcp.json"), "servers", true),
                    block_op(p.join(".github/copilot-instructions.md")),
                ]
            }
        }
        Client::Codex => {
            let agents = if user {
                env.codex_home.join("AGENTS.md")
            } else {
                p.join("AGENTS.md")
            };
            vec![
                Step::Op(Op {
                    path: env.codex_home.join("config.toml"),
                    kind: Kind::Toml {
                        section: toml_section(env),
                    },
                }),
                block_op(agents),
            ]
        }
        Client::Windsurf => {
            let rule = if user {
                Step::Skip {
                    what: "instruction",
                    reason: "no user-level AGENTS.md (run without --user)",
                }
            } else {
                block_op(p.join("AGENTS.md"))
            };
            vec![
                json_op(
                    h.join(".codeium/windsurf/mcp_config.json"),
                    "mcpServers",
                    false,
                ),
                rule,
            ]
        }
    }
}

/// What to do with a file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Update {
    Same,
    Write(String),
    /// Remove the file; `true`: keep a `.bak` copy first.
    Delete(bool),
    Skip(String),
}

fn bad(file: &Path, what: &str) -> CallgraphError {
    CallgraphError::Usage(format!("{}: {what} — left unchanged", file.display()))
}

fn parse_json(text: &str, file: &Path) -> Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(bad(
            file,
            crate::t!(
                "not a plain JSON object (comments?); add the snippet by hand",
                "nem sima JSON-objektum (megjegyzés?); a részletet add hozzá kézzel"
            ),
        )),
    }
}

fn render(map: &Map<String, Value>) -> Result<String> {
    Ok(serde_json::to_string_pretty(map)? + "\n")
}

pub(crate) fn json_set(text: &str, file: &Path, key: &str, entry: &Value) -> Result<Update> {
    let mut map = parse_json(text, file)?;
    let Value::Object(servers) = map.entry(key).or_insert_with(|| Value::Object(Map::new())) else {
        return Err(bad(file, &format!("\"{key}\" is not an object")));
    };
    if servers.get(SERVER) == Some(entry) {
        return Ok(Update::Same);
    }
    servers.insert(SERVER.to_owned(), entry.clone());
    Ok(Update::Write(render(&map)?))
}

pub(crate) fn json_remove(text: &str, file: &Path, key: &str) -> Result<Update> {
    let mut map = parse_json(text, file)?;
    let Some(Value::Object(servers)) = map.get_mut(key) else {
        return Ok(Update::Same);
    };
    if servers.shift_remove(SERVER).is_none() {
        return Ok(Update::Same);
    }
    if servers.is_empty() {
        map.shift_remove(key);
    }
    Ok(if map.is_empty() {
        Update::Delete(true)
    } else {
        Update::Write(render(&map)?)
    })
}

/// `(start, end)` of the marker block (end after the end marker).
fn block_range(text: &str, file: &Path) -> Result<Option<(usize, usize)>> {
    let Some(start) = text.find(BLOCK_START) else {
        return Ok(None);
    };
    match text[start..].find(BLOCK_END) {
        Some(at) => Ok(Some((start, start + at + BLOCK_END.len()))),
        None => Err(bad(file, &format!("{BLOCK_START} without {BLOCK_END}"))),
    }
}

/// `before` + `after` joined by one blank line; `None` when nothing is left.
fn join_rest(before: &str, after: &str) -> Option<String> {
    let (before, after) = (before.trim_end(), after.trim());
    match (before.is_empty(), after.is_empty()) {
        (true, true) => None,
        (true, false) => Some(format!("{after}\n")),
        (false, true) => Some(format!("{before}\n")),
        (false, false) => Some(format!("{before}\n\n{after}\n")),
    }
}

pub(crate) fn block_set(text: &str, file: &Path, body: &str) -> Result<Update> {
    let block = format!("{BLOCK_START}\n{body}\n{BLOCK_END}");
    Ok(match block_range(text, file)? {
        Some((s, e)) if text[s..e] == block => Update::Same,
        Some((s, e)) => Update::Write(format!("{}{block}{}", &text[..s], &text[e..])),
        None if text.trim().is_empty() => Update::Write(format!("{block}\n")),
        None => Update::Write(format!("{}\n\n{block}\n", text.trim_end())),
    })
}

pub(crate) fn block_remove(text: &str, file: &Path) -> Result<Update> {
    Ok(match block_range(text, file)? {
        None => Update::Same,
        Some((s, e)) => match join_rest(&text[..s], &text[e..]) {
            None => Update::Delete(true),
            Some(rest) => Update::Write(rest),
        },
    })
}

/// The normalized table name of a TOML header line (`[a."b"]` → `a.b`), `None` for any other line.
fn toml_header(line: &str) -> Option<String> {
    let line = line.split(" #").next().unwrap_or(line).trim();
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.chars().filter(|c| !matches!(c, '"' | ' ')).collect())
}

/// Byte range of the `[mcp_servers.callgraph]` table (up to the next table header; `with_sub`: its subtables too).
fn toml_range(text: &str, with_sub: bool) -> Option<(usize, usize)> {
    let main = format!("mcp_servers.{SERVER}");
    let sub = format!("{main}.");
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        if let Some(name) = toml_header(line) {
            if let Some(s) = start {
                if !(with_sub && name.starts_with(&sub)) {
                    return Some((s, offset));
                }
            } else if name == main {
                start = Some(offset);
            }
        }
        offset += line.len();
    }
    start.map(|s| (s, text.len()))
}

pub(crate) fn toml_set(text: &str, section: &str) -> Update {
    match toml_range(text, false) {
        Some((s, e)) => {
            let old = &text[s..e];
            if old.trim_end() == section.trim_end() {
                return Update::Same;
            }
            let trailing = &old[old.trim_end().len()..];
            let trailing = if trailing.is_empty() { "\n" } else { trailing };
            Update::Write(format!(
                "{}{}{trailing}{}",
                &text[..s],
                section.trim_end(),
                &text[e..]
            ))
        }
        None if text.trim().is_empty() => Update::Write(section.to_owned()),
        None => Update::Write(format!("{}\n\n{section}", text.trim_end())),
    }
}

pub(crate) fn toml_remove(text: &str) -> Update {
    match toml_range(text, true) {
        None => Update::Same,
        Some((s, e)) => match join_rest(&text[..s], &text[e..]) {
            None => Update::Delete(true),
            Some(rest) => Update::Write(rest),
        },
    }
}

pub(crate) fn file_set(existing: Option<&str>, content: &str) -> Update {
    match existing {
        None => Update::Write(content.to_owned()),
        Some(t) if t == content => Update::Same,
        Some(t) if t.contains(OWNED_MARK) => Update::Write(content.to_owned()),
        Some(t) if t.contains("callgraph") => Update::Skip(
            crate::t!(
                "a callgraph rule is already there",
                "már van callgraph-szabály"
            )
            .into(),
        ),
        Some(_) => Update::Skip(
            crate::t!(
                "file exists and was not created by setup",
                "a fájl létezik, nem a setup hozta létre"
            )
            .into(),
        ),
    }
}

pub(crate) fn file_remove(existing: Option<&str>, content: &str) -> Update {
    match existing {
        None => Update::Same,
        Some(t) if t.contains(OWNED_MARK) => Update::Delete(t != content),
        Some(_) => {
            Update::Skip(crate::t!("not created by setup", "nem a setup hozta létre").into())
        }
    }
}

fn compute(op: &Op, existing: Option<&str>, uninstall: bool) -> Result<Update> {
    let text = existing.unwrap_or("");
    Ok(match (&op.kind, uninstall) {
        (Kind::Json { key, entry }, false) => json_set(text, &op.path, key, entry)?,
        (Kind::Json { key, .. }, true) if existing.is_some() => json_remove(text, &op.path, key)?,
        (Kind::Toml { section }, false) => toml_set(text, section),
        (Kind::Toml { .. }, true) => toml_remove(text),
        (Kind::Block { body }, false) => block_set(text, &op.path, body)?,
        (Kind::Block { .. }, true) => block_remove(text, &op.path)?,
        (Kind::File { content }, false) => file_set(existing, content),
        (Kind::File { content }, true) => file_remove(existing, content),
        (_, true) => Update::Same,
    })
}

fn snippet(op: &Op) -> String {
    match &op.kind {
        Kind::Json { key, entry } => {
            serde_json::to_string_pretty(&json!({ *key: { SERVER: entry } })).unwrap_or_default()
        }
        Kind::Toml { section } => section.trim_end().to_owned(),
        Kind::Block { body } => format!("{BLOCK_START}\n{body}\n{BLOCK_END}"),
        Kind::File { content } => content.trim_end().to_owned(),
    }
}

#[derive(Debug, Clone)]
enum Outcome {
    Written(bool),
    Removed,
    Unchanged,
    Skipped(String),
    Failed(String),
}

fn read_opt(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn apply(op: &Op, uninstall: bool, dry: bool) -> Result<Outcome> {
    let existing = read_opt(&op.path)?;
    Ok(match compute(op, existing.as_deref(), uninstall)? {
        Update::Same => Outcome::Unchanged,
        Update::Skip(reason) => Outcome::Skipped(reason),
        Update::Write(text) => {
            if dry {
                Outcome::Written(false)
            } else {
                Outcome::Written(Settings::new(op.path.clone()).write(&text)?)
            }
        }
        Update::Delete(backup) => {
            if !dry {
                if backup {
                    let mut bak = op.path.as_os_str().to_owned();
                    bak.push(".bak");
                    fs::copy(&op.path, PathBuf::from(bak))?;
                }
                fs::remove_file(&op.path)?;
            }
            Outcome::Removed
        }
    })
}

fn show_path(path: &Path, env: &Env) -> String {
    if let Ok(rel) = path.strip_prefix(&env.project) {
        return rel.display().to_string();
    }
    match path.strip_prefix(&env.home) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Runs the setup; the exit code is 1 when any file failed.
pub fn run(opts: &Options, env: &Env, out: &mut dyn Write) -> Result<i32> {
    let clients: Vec<Client> = if opts.clients.is_empty() {
        Client::ALL
            .iter()
            .copied()
            .filter(|c| detect(*c, env))
            .collect()
    } else {
        opts.clients.clone()
    };
    if clients.is_empty() {
        writeln!(
            out,
            "{}",
            crate::t!(
                "No supported client found (Claude Code, Cursor, Codex CLI, VS Code, Windsurf). Name one: callgraph setup --client claude,cursor,codex,vscode,windsurf",
                "Nem találtam támogatott klienst (Claude Code, Cursor, Codex CLI, VS Code, Windsurf). Nevezz meg egyet: callgraph setup --client claude,cursor,codex,vscode,windsurf"
            )
        )?;
        return Ok(0);
    }
    let mut cache: HashMap<PathBuf, Outcome> = HashMap::new();
    let (mut failed, mut restart) = (false, Vec::new());
    for client in clients {
        writeln!(out, "{}", client.label())?;
        let mut touched = false;
        for step in plan(client, env, opts.user) {
            let op = match step {
                Step::Skip { what, reason } => {
                    writeln!(
                        out,
                        "  {what}: {}: {reason}",
                        crate::t!("skipped", "kihagyva")
                    )?;
                    continue;
                }
                Step::Op(op) => op,
            };
            let (outcome, fresh) = match cache.get(&op.path) {
                Some(o) => (o.clone(), false),
                None => {
                    let o = apply(&op, opts.uninstall, opts.dry_run)
                        .unwrap_or_else(|e| Outcome::Failed(e.to_string()));
                    cache.insert(op.path.clone(), o.clone());
                    (o, true)
                }
            };
            let name = show_path(&op.path, env);
            let line = match (&outcome, opts.dry_run) {
                (Outcome::Written(_), true) => crate::t!("would write", "írna").to_owned(),
                (Outcome::Written(true), false) => {
                    crate::tf!("written (backup {name}.bak)", "kiírva (mentés: {name}.bak)")
                }
                (Outcome::Written(false), false) => crate::t!("written", "kiírva").to_owned(),
                (Outcome::Removed, true) => crate::t!("would remove", "törölne").to_owned(),
                (Outcome::Removed, false) => crate::t!("removed", "eltávolítva").to_owned(),
                (Outcome::Unchanged, _) if opts.uninstall => {
                    crate::t!("not present", "nincs benne").to_owned()
                }
                (Outcome::Unchanged, _) => crate::t!("unchanged", "változatlan").to_owned(),
                (Outcome::Skipped(r), _) => format!("{}: {r}", crate::t!("skipped", "kihagyva")),
                (Outcome::Failed(e), _) => {
                    failed = true;
                    format!("{}: {e}", crate::t!("failed", "hiba"))
                }
            };
            writeln!(out, "  {name}: {line}")?;
            touched |= matches!(outcome, Outcome::Written(_) | Outcome::Removed);
            if opts.dry_run && fresh && matches!(outcome, Outcome::Written(_)) {
                for l in snippet(&op).lines() {
                    writeln!(out, "      {l}")?;
                }
            }
        }
        if touched {
            restart.push(client.label());
        }
    }
    if opts.dry_run {
        writeln!(
            out,
            "{}",
            crate::t!(
                "Dry run: nothing was written.",
                "Próbafutás: semmi nem íródott ki."
            )
        )?;
    } else if !restart.is_empty() {
        writeln!(
            out,
            "{}",
            crate::tf!(
                "Next: restart {}.",
                "Következő lépés: indítsd újra: {}.",
                restart.join(", ")
            )
        )?;
    }
    Ok(i32::from(failed))
}
