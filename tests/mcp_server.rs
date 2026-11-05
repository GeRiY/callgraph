//! `callgraph mcp`: valódi JSON-RPC munkamenet a bináris stdin/stdout-ján (initialize → tools/list → tools/call).
//! Hálózat nincs; a gyökér a repó (a gyerekfolyamat munkakönyvtára), a cél a `golden/samples` egy fájlja.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Session {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .arg("mcp")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("callgraph mcp starts");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// A következő válasz az adott `id`-re (az esetleges értesítéseket átugorja).
    fn response(&mut self, id: u64) -> Value {
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "server closed"
            );
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["id"] == json!(id) {
                return value;
            }
        }
    }

    fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}}));
        self.response(id)["result"].clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap_or_default()
}

#[test]
fn initialize_list_and_call_outline() {
    let mut s = Session::start();
    s.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}}),
    );
    let init = s.response(1);
    assert_eq!(init["result"]["serverInfo"]["name"], "callgraph");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    s.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    s.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let list = s.response(2);
    let mut names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "find_callers",
            "framework_view",
            "method_body",
            "notes",
            "outline",
            "projects",
            "search"
        ]
    );

    let outline = s.call(
        3,
        "outline",
        json!({"path": "golden/samples/swift/TodoStore.swift"}),
    );
    assert_ne!(outline["isError"], json!(true), "{outline}");
    let body = text(&outline);
    assert!(body.contains("TodoStore"), "{body}");
    assert!(body.contains("L"), "line ranges expected: {body}");

    // ugyanaz a szöveg, mint a CLI `show` kimenete
    let cli = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(["show", "--project", env!("CARGO_MANIFEST_DIR"), "--label"])
        .arg(env!("CARGO_MANIFEST_DIR"))
        .arg("golden/samples/swift/TodoStore.swift")
        .env("CALLGRAPH_LANG", "en")
        .output()
        .unwrap();
    let cli_text = String::from_utf8_lossy(&cli.stdout);
    let canonical = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    // Windowson a kiírt gyökér a `\\?\` előtag nélküli alak (lásd QueryEngine::relativize)
    let canonical = canonical.display().to_string();
    let plain = canonical
        .strip_prefix(r"\\?\")
        .unwrap_or(&canonical)
        .to_owned();
    assert_eq!(
        body.replace(&canonical, "<root>").replace(&plain, "<root>"),
        cli_text.replace(env!("CARGO_MANIFEST_DIR"), "<root>")
    );

    // a `method` sima stringként is jó (az LLM-kliensek gyakran így küldik); alapból tömör, `doc: true`-val docblockkal
    let single = s.call(
        4,
        "method_body",
        json!({"path": "golden/samples/swift/Todo.swift", "method": "toggled"}),
    );
    assert_ne!(single["isError"], json!(true), "{single}");
    let source = text(&single);
    assert!(source.contains("flipped.done.toggle()"), "{source}");
    assert!(!source.contains("A copy with"), "{source}");
    let documented = s.call(
        5,
        "method_body",
        json!({"path": "golden/samples/swift/Todo.swift", "method": ["toggled"], "doc": true}),
    );
    assert!(
        text(&documented).contains("    25      /// A copy with the done flag flipped."),
        "{documented}"
    );
}

#[test]
fn errors_are_tool_errors_and_the_server_keeps_running() {
    let mut s = Session::start();
    s.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}}),
    );
    s.response(1);
    s.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let escape = s.call(2, "outline", json!({"path": "../../etc/passwd"}));
    assert_eq!(escape["isError"], json!(true));
    assert!(text(&escape).contains("outside the project root"));

    let missing = s.call(
        3,
        "method_body",
        json!({"path": "golden/samples/swift/Todo.swift", "method": ["nope"]}),
    );
    assert_eq!(missing["isError"], json!(true));
    assert!(text(&missing).contains("No such method"), "{missing}");

    let depth = s.call(4, "find_callers", json!({"pattern": "x", "depth": 9}));
    assert_eq!(depth["isError"], json!(true));

    let alias = s.call(5, "outline", json!({"path": "src", "project": "nope"}));
    assert_eq!(alias["isError"], json!(true));
    assert!(text(&alias).contains("Unknown project alias"));

    let callers = s.call(
        6,
        "find_callers",
        json!({"pattern": "TodoStore", "path": "golden/samples/swift"}),
    );
    assert_ne!(callers["isError"], json!(true), "{callers}");
}
