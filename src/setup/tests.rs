//! `callgraph setup`: JSON / TOML / marker-block merge, idempotency, uninstall, dry-run, client selection.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("callgraph-setup-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn written(update: Update) -> String {
    match update {
        Update::Write(text) => text,
        other => panic!("expected a write, got {other:?}"),
    }
}

fn entry() -> Value {
    json!({"command": "/opt/cg/callgraph", "args": ["mcp"]})
}

fn file() -> &'static Path {
    Path::new("x.json")
}

#[test]
fn json_set_keeps_existing_servers_and_is_idempotent() {
    let before = r#"{"theme": "dark", "mcpServers": {"other": {"command": "x"}}}"#;
    let after = written(json_set(before, file(), "mcpServers", &entry()).unwrap());
    let value: Value = serde_json::from_str(&after).unwrap();
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["mcpServers"]["other"], json!({"command": "x"}));
    assert_eq!(value["mcpServers"]["callgraph"], entry());
    assert_eq!(
        json_set(&after, file(), "mcpServers", &entry()).unwrap(),
        Update::Same
    );
    // a stale path is replaced in place
    let stale = r#"{"mcpServers": {"callgraph": {"command": "/old", "args": ["mcp"]}}}"#;
    let fixed = written(json_set(stale, file(), "mcpServers", &entry()).unwrap());
    assert!(fixed.contains("/opt/cg/callgraph") && !fixed.contains("/old"));
}

#[test]
fn json_set_creates_from_empty_and_rejects_foreign_shapes() {
    let created = written(json_set("", file(), "servers", &entry()).unwrap());
    assert!(created.contains("\"servers\"") && created.ends_with('\n'));
    assert!(json_set("[1]", file(), "servers", &entry()).is_err());
    assert!(json_set("{\"servers\": 3}", file(), "servers", &entry()).is_err());
    assert!(json_set("{ // comment\n}", file(), "servers", &entry()).is_err());
}

#[test]
fn json_remove_only_drops_callgraph() {
    let both = r#"{"mcpServers": {"other": {"command": "x"}, "callgraph": {"command": "y"}}}"#;
    let after = written(json_remove(both, file(), "mcpServers").unwrap());
    let value: Value = serde_json::from_str(&after).unwrap();
    assert!(value["mcpServers"].get("callgraph").is_none());
    assert!(value["mcpServers"].get("other").is_some());
    assert_eq!(
        json_remove(&after, file(), "mcpServers").unwrap(),
        Update::Same
    );
    let only = r#"{"mcpServers": {"callgraph": {"command": "y"}}}"#;
    assert_eq!(
        json_remove(only, file(), "mcpServers").unwrap(),
        Update::Delete(true)
    );
    let extra = r#"{"a": 1, "mcpServers": {"callgraph": {"command": "y"}}}"#;
    let kept: Value =
        serde_json::from_str(&written(json_remove(extra, file(), "mcpServers").unwrap())).unwrap();
    assert_eq!(kept, json!({"a": 1}));
}

#[test]
fn block_insert_replace_remove() {
    let f = Path::new("AGENTS.md");
    let inserted = written(block_set("# Mine\n\nText.\n", f, "one").unwrap());
    assert!(
        inserted.starts_with(
            "# Mine\n\nText.\n\n<!-- callgraph:start -->\none\n<!-- callgraph:end -->\n"
        )
    );
    assert_eq!(block_set(&inserted, f, "one").unwrap(), Update::Same);
    let replaced = written(block_set(&format!("{inserted}\n## After\n"), f, "two").unwrap());
    assert!(replaced.contains("\ntwo\n") && !replaced.contains("one"));
    assert!(replaced.starts_with("# Mine") && replaced.ends_with("## After\n"));
    let removed = written(block_remove(&replaced, f).unwrap());
    assert_eq!(removed, "# Mine\n\nText.\n\n## After\n");
    assert_eq!(block_remove(&removed, f).unwrap(), Update::Same);
    // only our block in the file: the file goes away
    let fresh = written(block_set("", f, "one").unwrap());
    assert_eq!(block_remove(&fresh, f).unwrap(), Update::Delete(true));
    // a start marker without an end marker is refused, not guessed
    assert!(block_set("<!-- callgraph:start -->\nx", f, "one").is_err());
}

#[test]
fn toml_set_replace_remove() {
    let section = "[mcp_servers.callgraph]\ncommand = \"/opt/cg\"\nargs = [\"mcp\"]\n";
    let existing = "model = \"o3\"\n\n[mcp_servers.other]\ncommand = \"x\"\n";
    let added = written(toml_set(existing, section));
    assert!(added.starts_with(existing.trim_end()) && added.ends_with(section));
    assert_eq!(toml_set(&added, section), Update::Same);
    // stale entry in the middle is replaced, neighbours stay
    let stale = "[mcp_servers.callgraph]\ncommand = \"/old\"\nargs = [\"mcp\"]\n\n[other]\na = 1\n";
    let fixed = written(toml_set(stale, section));
    assert_eq!(fixed, format!("{}\n[other]\na = 1\n", section));
    assert_eq!(toml_set(&fixed, section), Update::Same);
    // remove: ours (and its subtables) go, the rest stays
    let with_sub = format!("{section}[mcp_servers.callgraph.env]\nK = \"v\"\n\n[other]\na = 1\n");
    assert_eq!(written(toml_remove(&with_sub)), "[other]\na = 1\n");
    assert_eq!(written(toml_remove(&added)), existing);
    assert_eq!(toml_remove(existing), Update::Same);
    assert_eq!(toml_remove(section), Update::Delete(true));
    // Windows path escaping
    assert!(toml_str("C:\\a\\b").contains("C:\\\\a\\\\b"));
}

#[test]
fn rule_files_are_owned_by_marker() {
    let mine = md_rule();
    assert_eq!(file_set(None, &mine), Update::Write(mine.clone()));
    assert_eq!(file_set(Some(&mine), &mine), Update::Same);
    assert!(matches!(
        file_set(Some("use .claude/skills/callgraph/callgraph.sh"), &mine),
        Update::Skip(_)
    ));
    assert!(matches!(
        file_set(Some("unrelated"), &mine),
        Update::Skip(_)
    ));
    assert_eq!(file_remove(Some(&mine), &mine), Update::Delete(false));
    assert!(matches!(
        file_remove(Some("unrelated"), &mine),
        Update::Skip(_)
    ));
    assert!(cursor_rule().contains("alwaysApply: true"));
}

fn env(tmp: &TempDir) -> Env {
    let project = tmp.0.join("proj");
    let home = tmp.0.join("home");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&home).unwrap();
    Env {
        project,
        config_dir: home.join(".config"),
        codex_home: home.join(".codex"),
        home,
        exe: PathBuf::from("/opt/cg/callgraph"),
        path_dirs: Vec::new(),
    }
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            all.extend(files(&path));
        } else {
            all.push(path);
        }
    }
    all.sort();
    all
}

fn opts(clients: &[Client]) -> Options {
    Options {
        clients: clients.to_vec(),
        ..Options::default()
    }
}

fn go(opts: &Options, env: &Env) -> String {
    let mut out = Vec::new();
    run(opts, env, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn dry_run_writes_nothing_and_shows_the_snippet() {
    let tmp = TempDir::new();
    let env = env(&tmp);
    let mut o = opts(&Client::ALL);
    o.dry_run = true;
    let text = go(&o, &env);
    assert!(
        files(&tmp.0).is_empty(),
        "dry run wrote: {:?}",
        files(&tmp.0)
    );
    assert!(text.contains("\"mcpServers\"") && text.contains("/opt/cg/callgraph"));
    assert!(text.contains("[mcp_servers.callgraph]") && text.contains("callgraph:start"));
}

#[test]
fn install_twice_changes_nothing_and_backs_up_once() {
    let tmp = TempDir::new();
    let env = env(&tmp);
    fs::write(
        env.project.join(".mcp.json"),
        r#"{"mcpServers": {"other": {"command": "x"}}}"#,
    )
    .unwrap();
    let o = opts(&Client::ALL);
    let first = go(&o, &env);
    assert!(first.contains("Next: restart") || first.contains("indítsd újra"));
    assert!(env.project.join(".mcp.json.bak").is_file());
    let mcp: Value =
        serde_json::from_str(&fs::read_to_string(env.project.join(".mcp.json")).unwrap()).unwrap();
    assert!(mcp["mcpServers"]["other"].is_object() && mcp["mcpServers"]["callgraph"].is_object());
    for rel in [
        ".claude/rules/callgraph.md",
        ".cursor/mcp.json",
        ".cursor/rules/callgraph.mdc",
        ".vscode/mcp.json",
        ".github/copilot-instructions.md",
        "AGENTS.md",
    ] {
        assert!(env.project.join(rel).is_file(), "missing {rel}");
    }
    assert!(env.home.join(".codex/config.toml").is_file());
    assert!(env.home.join(".codeium/windsurf/mcp_config.json").is_file());
    let snapshot: Vec<(PathBuf, String)> = files(&tmp.0)
        .into_iter()
        .map(|p| (p.clone(), fs::read_to_string(&p).unwrap()))
        .collect();
    let second = go(&o, &env);
    assert!(!second.contains("written"), "{second}");
    let again: Vec<(PathBuf, String)> = files(&tmp.0)
        .into_iter()
        .map(|p| (p.clone(), fs::read_to_string(&p).unwrap()))
        .collect();
    assert_eq!(snapshot, again);
}

#[test]
fn only_selected_clients_are_touched() {
    let tmp = TempDir::new();
    let env = env(&tmp);
    go(&opts(&[Client::Cursor]), &env);
    let names: Vec<String> = files(&tmp.0)
        .iter()
        .map(|p| p.strip_prefix(&tmp.0).unwrap().display().to_string())
        .collect();
    assert!(names.iter().all(|n| n.contains(".cursor")), "{names:?}");
}

#[test]
fn detection_uses_config_dirs_and_path() {
    let tmp = TempDir::new();
    let mut env = env(&tmp);
    fs::create_dir_all(env.home.join(".cursor")).unwrap();
    let found: Vec<Client> = Client::ALL
        .iter()
        .copied()
        .filter(|c| detect(*c, &env))
        .collect();
    assert_eq!(found, vec![Client::Cursor]);
    let bin = tmp.0.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(bin.join("codex"), "").unwrap();
    env.path_dirs.push(bin);
    assert!(detect(Client::Codex, &env));
    assert!(!detect(Client::Windsurf, &env));
    // nothing selected and nothing found: a hint, no files
    let tmp2 = TempDir::new();
    let text = go(&Options::default(), &self::env(&tmp2));
    assert!(text.contains("--client"));
    assert!(files(&tmp2.0).is_empty());
}

#[test]
fn uninstall_removes_only_ours() {
    let tmp = TempDir::new();
    let env = env(&tmp);
    fs::write(
        env.project.join(".mcp.json"),
        r#"{"mcpServers": {"other": {"command": "x"}}}"#,
    )
    .unwrap();
    fs::write(env.project.join("AGENTS.md"), "# Mine\n").unwrap();
    fs::create_dir_all(env.home.join(".codex")).unwrap();
    fs::write(env.home.join(".codex/config.toml"), "model = \"o3\"\n").unwrap();
    let o = opts(&[Client::Claude, Client::Codex]);
    go(&o, &env);
    let mut u = o.clone();
    u.uninstall = true;
    go(&u, &env);
    let mcp: Value =
        serde_json::from_str(&fs::read_to_string(env.project.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp, json!({"mcpServers": {"other": {"command": "x"}}}));
    assert_eq!(
        fs::read_to_string(env.project.join("AGENTS.md")).unwrap(),
        "# Mine\n"
    );
    assert_eq!(
        fs::read_to_string(env.home.join(".codex/config.toml")).unwrap(),
        "model = \"o3\"\n"
    );
    assert!(!env.project.join(".claude/rules/callgraph.md").exists());
    // a rule file that is not ours survives
    fs::create_dir_all(env.project.join(".claude/rules")).unwrap();
    fs::write(
        env.project.join(".claude/rules/callgraph.md"),
        "my own rule",
    )
    .unwrap();
    go(&u, &env);
    assert_eq!(
        fs::read_to_string(env.project.join(".claude/rules/callgraph.md")).unwrap(),
        "my own rule"
    );
}
