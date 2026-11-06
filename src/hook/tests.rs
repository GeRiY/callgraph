//! A hook bemenetének értelmezése (Edit / Write / NotebookEdit JSON, hibás / hiányzó mezők → nincs teendő), a
//! projekt-gyökér keresése, és a `settings.json` szerkesztése (üres / hiányzó fájl, meglévő hookok maradnak,
//! idempotens, eltávolítás).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{
    Settings, edited_file, hook_command, install, is_installed, project_root_for, uninstall,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("callgraph-hook-{}-{n}", std::process::id()));
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

const FILE: &str = "settings.json";
const COMMAND: &str = "\"/opt/skill/bin/callgraph\" hook post-edit";

fn abs(path: &str) -> PathBuf {
    std::env::temp_dir().join(path)
}

#[test]
fn reads_the_edited_file_from_edit_write_and_notebook_payloads() {
    let file = abs("p/a.php");
    let edit = json!({
        "session_id": "s", "hook_event_name": "PostToolUse", "tool_name": "Edit", "cwd": "/ignored",
        "tool_input": { "file_path": file, "old_string": "a", "new_string": "b" },
        "tool_response": { "success": true }
    });
    assert_eq!(edited_file(&edit.to_string()), Some(file.clone()));
    let write =
        json!({ "tool_name": "Write", "tool_input": { "file_path": file, "content": "<?php\n" } });
    assert_eq!(edited_file(&write.to_string()), Some(file.clone()));
    let notebook = json!({ "tool_name": "NotebookEdit", "tool_input": { "notebook_path": file } });
    assert_eq!(edited_file(&notebook.to_string()), Some(file.clone()));
    // relatív út: a cwd-hez képest
    let base = abs("p");
    let relative = json!({ "cwd": base, "tool_input": { "file_path": "src/b.php" } });
    assert_eq!(
        edited_file(&relative.to_string()),
        Some(base.join("src/b.php"))
    );
}

#[test]
fn odd_or_missing_fields_mean_no_op() {
    for payload in [
        "",
        "not json",
        "[]",
        "{}",
        r#"{"tool_input": null}"#,
        r#"{"tool_input": {}}"#,
        r#"{"tool_input": {"file_path": 42}}"#,
        r#"{"tool_input": {"file_path": "  "}}"#,
        r#"{"tool_input": {"file_path": "rel.php"}}"#,
        r#"{"cwd": "also/relative", "tool_input": {"file_path": "rel.php"}}"#,
        r#"{"tool_input": {"command": "ls"}}"#,
    ] {
        assert_eq!(edited_file(payload), None, "{payload}");
    }
}

#[test]
fn the_project_root_is_a_registered_root_or_a_dir_with_a_store() {
    let dir = TempDir::new();
    let registered = dir.0.join("reg");
    let stored = dir.0.join("stored");
    fs::create_dir_all(registered.join("src/deep")).unwrap();
    fs::create_dir_all(stored.join(".callgraph")).unwrap();
    fs::create_dir_all(stored.join("lib")).unwrap();
    fs::create_dir_all(dir.0.join("plain/src")).unwrap();
    let roots = vec![registered.clone()];
    assert_eq!(
        project_root_for(&registered.join("src/deep/a.php"), &roots),
        Some(registered.clone())
    );
    // a fájlnak nem kell léteznie, a szülőjének sem
    assert_eq!(
        project_root_for(&stored.join("lib/new/b.php"), &roots),
        Some(stored.clone())
    );
    assert_eq!(
        project_root_for(&dir.0.join("plain/src/c.php"), &roots),
        None
    );
}

fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn install_into_a_missing_or_empty_file() {
    for text in ["", "  \n", "{}"] {
        let out = install(text, Path::new(FILE), COMMAND).unwrap().unwrap();
        assert_eq!(
            parsed(&out),
            json!({ "hooks": { "PostToolUse": [ {
                "matcher": "Edit|Write|MultiEdit",
                "hooks": [ { "type": "command", "command": COMMAND, "timeout": 10 } ]
            } ] } })
        );
        assert!(is_installed(&out, Path::new(FILE)).unwrap());
        // idempotens
        assert_eq!(install(&out, Path::new(FILE), COMMAND).unwrap(), None);
    }
}

#[test]
fn install_keeps_other_settings_and_hooks_and_uninstall_restores_them() {
    let original = json!({
        "model": "opus",
        "permissions": { "allow": ["Bash(ls)"] },
        "hooks": {
            "PreToolUse": [ { "matcher": "Bash", "hooks": [ { "type": "command", "command": "guard.sh" } ] } ],
            "PostToolUse": [ { "matcher": "Write", "hooks": [ { "type": "command", "command": "fmt.sh" } ] } ]
        }
    });
    let text = serde_json::to_string_pretty(&original).unwrap();
    assert!(!is_installed(&text, Path::new(FILE)).unwrap());
    let installed = install(&text, Path::new(FILE), COMMAND).unwrap().unwrap();
    let value = parsed(&installed);
    assert_eq!(value["model"], "opus");
    assert_eq!(value["permissions"], original["permissions"]);
    assert_eq!(
        value["hooks"]["PreToolUse"],
        original["hooks"]["PreToolUse"]
    );
    let post = value["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(post.len(), 2);
    assert_eq!(post[0], original["hooks"]["PostToolUse"][0]);
    assert_eq!(post[1]["hooks"][0]["command"], COMMAND);
    // a kulcsok sorrendje marad
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["model", "permissions", "hooks"]);

    // egy másik bináris-út: a régi bejegyzés helyére kerül (nem duplikálódik)
    let moved = "\"/new/bin/callgraph\" hook post-edit";
    let again = parsed(
        &install(&installed, Path::new(FILE), moved)
            .unwrap()
            .unwrap(),
    );
    let post = again["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(post.len(), 2);
    assert_eq!(post[1]["hooks"][0]["command"], moved);

    let removed = uninstall(&installed, Path::new(FILE)).unwrap().unwrap();
    assert_eq!(parsed(&removed), original);
    assert!(!is_installed(&removed, Path::new(FILE)).unwrap());
    assert_eq!(uninstall(&removed, Path::new(FILE)).unwrap(), None);
}

#[test]
fn uninstall_drops_emptied_sections_and_keeps_shared_groups() {
    let installed = install("{}", Path::new(FILE), COMMAND).unwrap().unwrap();
    assert_eq!(
        parsed(&uninstall(&installed, Path::new(FILE)).unwrap().unwrap()),
        json!({})
    );
    // a saját hook egy közös csoportban: csak az kerül ki
    let shared = json!({ "hooks": { "PostToolUse": [ { "matcher": "Edit", "hooks": [
        { "type": "command", "command": COMMAND },
        { "type": "command", "command": "other.sh" }
    ] } ] } });
    let out = uninstall(&shared.to_string(), Path::new(FILE))
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed(&out),
        json!({ "hooks": { "PostToolUse": [ { "matcher": "Edit", "hooks": [
            { "type": "command", "command": "other.sh" }
        ] } ] } })
    );
}

#[test]
fn invalid_settings_are_left_alone() {
    for text in [
        "not json",
        "[1]",
        r#"{"hooks": []}"#,
        r#"{"hooks": {"PostToolUse": {}}}"#,
    ] {
        assert!(install(text, Path::new(FILE), COMMAND).is_err(), "{text}");
        assert!(uninstall(text, Path::new(FILE)).is_err(), "{text}");
    }
}

#[test]
fn settings_file_is_backed_up_and_written_atomically() {
    let dir = TempDir::new();
    let settings = Settings::new(dir.0.join("claude/settings.json"));
    assert_eq!(settings.read().unwrap(), "");
    // új fájl: nincs mit menteni
    assert!(!settings.write("{}\n").unwrap());
    assert!(!settings.backup_file().exists());
    assert!(settings.write("{\"a\": 1}\n").unwrap());
    assert_eq!(fs::read_to_string(settings.backup_file()).unwrap(), "{}\n");
    assert_eq!(settings.read().unwrap(), "{\"a\": 1}\n");
    assert!(settings.backup_file().ends_with("settings.json.bak"));
}

#[test]
fn the_hook_command_quotes_the_binary_path() {
    let command = hook_command(Path::new("/opt/my skill/bin/callgraph"));
    assert_eq!(command, "\"/opt/my skill/bin/callgraph\" hook post-edit");
    assert_eq!(
        hook_command(Path::new("/x/$HOME/callgraph")),
        "\"/x/\\$HOME/callgraph\" hook post-edit"
    );
}
