//! A `PreToolUse` hook (`callgraph hook pre-nudge`): egy Grep / Read / Bash-grep hívás előtt, ha a kérés
//! kódazonosítónak vagy egy teljes forrásfájl olvasásának látszik egy callgraph-indexes projektben, egy rövid,
//! NEM blokkoló tippet ad a modellnek (`hookSpecificOutput.additionalContext`): `locate` / vázlat / `--body`.
//! A döntés tiszta függvény (tesztelhető); a projekt-ellenőrzés és a fájlméret a hívótól jön.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// Ennél kisebb fájlt nem érdemes vázlattal kiváltani (a teljes olvasás is olcsó).
pub(crate) const MIN_FILE_BYTES: u64 = 4096;

/// Egy tipp: a kulcs (az ismétlés-szűréshez) és a modellnek szánt szöveg.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Nudge {
    pub(crate) key: String,
    pub(crate) context: String,
}

/// A hook kimenete: a dokumentált `PreToolUse` séma, `permissionDecision` nélkül (a szokásos engedélyezés marad).
pub(crate) fn render(context: &str) -> String {
    json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": context}})
        .to_string()
}

/// A hook JSON-jából a tipp; `None`: nincs teendő (más eszköz, nem azonosító-minta, nem ismert projekt, hibás JSON).
/// `in_project`: a megadott mappa callgraph-indexes projektben van-e; `big_code_file`: elég nagy, elemzett kódfájl-e.
pub(crate) fn decide(
    json: &str,
    script: &str,
    in_project: &dyn Fn(&Path) -> bool,
    big_code_file: &dyn Fn(&Path) -> bool,
) -> Option<Nudge> {
    let value: Value = serde_json::from_str(json).ok()?;
    let input = value.get("tool_input")?;
    let cwd = value.get("cwd").and_then(Value::as_str).map(PathBuf::from);
    match value.get("tool_name")?.as_str()? {
        "Grep" => {
            let name = identifier_pattern(input.get("pattern")?.as_str()?)?;
            let base = input
                .get("path")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or(cwd)?;
            if base.is_file() || !in_project(&base) {
                return None;
            }
            Some(grep_nudge(script, name))
        }
        "Bash" => {
            let name = bash_grep_identifier(input.get("command")?.as_str()?)?;
            if !in_project(&cwd?) {
                return None;
            }
            Some(grep_nudge(script, name))
        }
        "Read" => {
            if input.get("offset").is_some_and(|v| !v.is_null())
                || input.get("limit").is_some_and(|v| !v.is_null())
            {
                return None;
            }
            let file = PathBuf::from(input.get("file_path")?.as_str()?);
            let file = if file.is_absolute() {
                file
            } else {
                cwd?.join(file)
            };
            if !big_code_file(&file) {
                return None;
            }
            Some(Nudge {
                key: format!("read:{}", file.display()),
                context: format!(
                    "callgraph: `{script} {f}` is this file's outline (classes, methods, line ranges); `{script} {f}::<method> --body` prints one method. Use Read with offset/limit for exact lines.",
                    f = file.display()
                ),
            })
        }
        _ => None,
    }
}

fn grep_nudge(script: &str, name: &str) -> Nudge {
    Nudge {
        key: format!("grep:{name}"),
        context: format!(
            "callgraph: `{script} locate {name}` ranks the files for this name with reasons; `{script} <file>` gives the outline, `{script} <file>::<method> --body` one method."
        ),
    }
}

/// A minta kódazonosító (`foo`, `Foo::bar`, `a.b`, `a->b`) vagy `function|class|def|fn|struct|trait|interface|enum|type
/// <azonosító>` alakú-e; az azonosítót adja vissza.
pub(crate) fn identifier_pattern(pattern: &str) -> Option<&str> {
    let pattern = pattern.trim();
    let name = match pattern.split_once(' ') {
        Some((
            "function" | "class" | "def" | "fn" | "struct" | "trait" | "interface" | "enum"
            | "type",
            rest,
        )) => rest.trim(),
        _ => pattern,
    };
    is_identifier(name).then_some(name)
}

fn is_identifier(name: &str) -> bool {
    let ok_first = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    ok_first
        && (3..=80).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-' | '>'))
        && !name.ends_with(['-', '.', ':'])
}

/// A `grep` / `rg` hívás mintája, ha a parancs (vagy egy `&&` / `;` szerinti tagja) így kezdődik, és a minta azonosító.
/// A csővezeték közepén álló grep (egy másik parancs kimenetének szűrése) nem számít.
fn bash_grep_identifier(command: &str) -> Option<&str> {
    for part in command.split("&&").flat_map(|p| p.split(';')) {
        let first = part.split('|').next().unwrap_or("");
        let mut words = first.split_whitespace();
        if !matches!(words.next(), Some("grep" | "rg" | "egrep")) {
            continue;
        }
        let mut pattern = None;
        while let Some(word) = words.next() {
            if matches!(word, "-e" | "--regexp") {
                pattern = words.next();
                break;
            }
            if matches!(
                word,
                "-A" | "-B"
                    | "-C"
                    | "-g"
                    | "-t"
                    | "-T"
                    | "-m"
                    | "-f"
                    | "--glob"
                    | "--type"
                    | "--include"
                    | "--exclude"
                    | "--max-count"
            ) {
                words.next();
                continue;
            }
            if word.starts_with('-') {
                continue;
            }
            pattern = Some(word);
            break;
        }
        let pattern = pattern?.trim_matches(['"', '\'']);
        if let Some(name) = identifier_pattern(pattern) {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "bash /s/callgraph.sh";

    fn run(json: &str) -> Option<Nudge> {
        decide(json, S, &|_| true, &|_| true)
    }

    fn grep(pattern: &str) -> String {
        json!({"tool_name": "Grep", "tool_input": {"pattern": pattern}, "cwd": "/p"}).to_string()
    }

    #[test]
    fn identifier_grep_nudges_toward_locate() {
        for p in [
            "handleRequest",
            "Foo::bar",
            "user.name",
            "a->b",
            "function foo_bar",
            "class Foo",
            "def parse",
        ] {
            let n = run(&grep(p)).unwrap_or_else(|| panic!("{p}"));
            assert!(n.context.contains("locate "), "{p}");
        }
        let n = run(&grep("class Foo")).unwrap();
        assert!(n.context.contains("locate Foo`"));
    }

    #[test]
    fn regex_or_text_grep_is_silent() {
        for p in [
            "foo.*bar",
            "TODO: fix this",
            "\\bfoo\\b",
            "^fn ",
            "a|b",
            "ab",
            "",
        ] {
            assert_eq!(run(&grep(p)), None, "{p}");
        }
    }

    #[test]
    fn read_nudges_only_without_offset_or_limit() {
        let read = |input: Value| {
            run(&json!({"tool_name": "Read", "tool_input": input, "cwd": "/p"}).to_string())
        };
        assert!(read(json!({"file_path": "/p/src/a.rs"})).is_some());
        assert!(read(json!({"file_path": "src/a.rs"})).is_some());
        assert_eq!(read(json!({"file_path": "/p/a.rs", "offset": 10})), None);
        assert_eq!(read(json!({"file_path": "/p/a.rs", "limit": 50})), None);
        let small = decide(
            &json!({"tool_name": "Read", "tool_input": {"file_path": "/p/a.rs"}}).to_string(),
            S,
            &|_| true,
            &|_| false,
        );
        assert_eq!(small, None);
    }

    #[test]
    fn bash_grep_and_rg_nudge_but_filters_do_not() {
        let bash = |c: &str| {
            run(
                &json!({"tool_name": "Bash", "tool_input": {"command": c}, "cwd": "/p"})
                    .to_string(),
            )
        };
        assert!(bash("grep -rn handleRequest src").is_some());
        assert!(bash("rg -t rust \"handleRequest\"").is_some());
        assert!(bash("cd x && rg -e Foo::bar .").is_some());
        assert_eq!(bash("cat log | grep handleRequest"), None);
        assert_eq!(bash("grep -rn 'foo.*bar' src"), None);
        assert_eq!(bash("ls -la"), None);
    }

    #[test]
    fn no_index_other_tools_and_bad_json_are_silent() {
        assert_eq!(
            decide(&grep("handleRequest"), S, &|_| false, &|_| true),
            None
        );
        assert_eq!(
            run(r#"{"tool_name":"Edit","tool_input":{"pattern":"handleRequest"}}"#),
            None
        );
        for bad in ["", "{", "[]", "null", r#"{"tool_name":1}"#] {
            assert_eq!(run(bad), None, "{bad}");
        }
    }

    #[test]
    fn output_follows_the_pre_tool_use_schema() {
        let out: Value = serde_json::from_str(&render("hi")).unwrap();
        assert_eq!(
            out,
            json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "hi"}})
        );
    }
}
