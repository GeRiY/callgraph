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
        Self::start_in(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
    }

    /// A szerver a megadott munkakönyvtárral (ez a projekt-gyökér alias nélkül).
    fn start_in(dir: &std::path::Path) -> Self {
        Self::start_with(dir, &[])
    }

    /// Mint a `start_in`, további környezeti változókkal (pl. `CALLGRAPH_STATS_DIR`).
    fn start_with(dir: &std::path::Path, env: &[(&str, &std::ffi::OsStr)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .arg("mcp")
            .current_dir(dir)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .envs(env.iter().copied())
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
    // a használati útmutató a SKILL-forrásból, MCP-nevekkel (a CLI-kapcsolók nélkül)
    let instructions = init["result"]["instructions"].as_str().unwrap_or_default();
    for item in [
        "find_callers",
        "(^|::|\\.|->)",
        "`root: ",
        "`via ",
        "`raw: true`",
        "`guide`",
    ] {
        assert!(instructions.contains(item), "{item}: {instructions}");
    }
    assert!(!instructions.contains("`--"), "{instructions}");
    // a munkakönyvtár (a repó) tömör projekt-profilja az útmutató végén
    assert!(instructions.contains("`project_profile`"), "{instructions}");
    assert!(instructions.contains("\nprofile: "), "{instructions}");
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
            "guide",
            "locate",
            "method_body",
            "next",
            "notes",
            "outline",
            "project_profile",
            "projects",
            "search"
        ]
    );
    // minden eszköz csak olvas: a négy annotáció explicit (katalógusok, pl. az OpenAI-é, enélkül elutasítják)
    for tool in list["result"]["tools"].as_array().unwrap() {
        let a = &tool["annotations"];
        assert_eq!(
            (
                &a["readOnlyHint"],
                &a["destructiveHint"],
                &a["idempotentHint"],
                &a["openWorldHint"]
            ),
            (&json!(true), &json!(false), &json!(true), &json!(false)),
            "{tool}"
        );
    }

    // `guide`: a teljes SKILL.md a beállított nyelven, frontmatter és jelölők nélkül
    let guide = s.call(30, "guide", json!({}));
    assert_ne!(guide["isError"], json!(true), "{guide}");
    let manual = text(&guide);
    assert!(
        manual.starts_with("# callgraph — code map without Read\n"),
        "{manual}"
    );
    assert!(manual.contains("## MCP server"), "{manual}");
    assert!(!manual.contains("<!--"), "{manual}");

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

/// Az MCP-szerver a projekt-gráfot kérésről kérésre a memóriában tartja: egy közben szerkesztett fájl a következő
/// kérésben látszik, és a válasz azonos a CLI-ével.
#[test]
fn the_server_keeps_the_project_graph_and_sees_edits() {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("callgraph-mcp-graph-{}-{id}", std::process::id()));
    let files = [
        (
            "src/Repo.php",
            "<?php\nnamespace App;\n\nclass Repo\n{\n    public function find(int $id): array\n    {\n        return [];\n    }\n}\n",
        ),
        (
            "src/Lookup.php",
            "<?php\nnamespace App;\n\nclass Lookup\n{\n    private Repo $repo;\n\n    public function run(int $id): array\n    {\n        return $this->repo->find($id);\n    }\n}\n",
        ),
    ];
    for (path, body) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    let dir = dir.canonicalize().unwrap();
    let mut s = Session::start_in(&dir);
    s.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}}),
    );
    s.response(1);
    s.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let args = json!({"pattern": "Repo::find", "depth": 2});
    let first = s.call(2, "find_callers", args.clone());
    assert!(text(&first).contains("Lookup::run()"), "{first}");
    assert!(!text(&first).contains("Admin::show()"), "{first}");
    std::fs::write(
        dir.join("src/Admin.php"),
        "<?php\nnamespace App;\n\nclass Admin\n{\n    private Lookup $lookup;\n\n    public function show(int $id): array\n    {\n        return $this->lookup->run($id);\n    }\n}\n",
    )
    .unwrap();
    let second = s.call(3, "find_callers", args);
    let body = text(&second).to_owned();
    assert!(
        body.contains("Admin::show()  L8-11  ← App\\Lookup::run"),
        "{second}"
    );
    let cli = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(["show", "--project"])
        .arg(&dir)
        .arg("--label")
        .arg(dir.display().to_string())
        .args(["--grep", "Repo::find", "--depth", "2", "."])
        .env("CALLGRAPH_LANG", "en")
        .output()
        .unwrap();
    assert_eq!(body, String::from_utf8_lossy(&cli.stdout));
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `locate`: a feladat nevei egy hívásban → rangsorolt fájlok indoklással, a CLI-vel azonos szöveggel; üres
/// kifejezés-lista eszköz-hiba.
#[test]
fn locate_ranks_the_starting_files_like_the_cli() {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("callgraph-mcp-locate-{}-{id}", std::process::id()));
    let files = [
        (
            "src/RefundService.php",
            "<?php\nnamespace App;\n\nclass RefundService\n{\n    public function processRefund(int $id): void\n    {\n    }\n}\n",
        ),
        (
            "src/Order.php",
            "<?php\nnamespace App;\n\nclass Order\n{\n    public function paid(): string\n    {\n        return 'payment_status';\n    }\n}\n",
        ),
    ];
    for (path, body) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    let dir = dir.canonicalize().unwrap();
    let mut s = Session::start_in(&dir);
    s.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}}),
    );
    s.response(1);
    s.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let args = json!({"terms": ["processRefund", "payment_status", "nothingLikeThis"]});
    let found = s.call(2, "locate", args.clone());
    assert_ne!(found["isError"], json!(true), "{found}");
    // Windowson az út-elválasztó `\`
    let body = text(&found).replace('\\', "/");
    assert!(body.contains("locate: 3 terms · "), "{body}");
    assert!(body.contains("1. src/RefundService.php"), "{body}");
    assert!(body.contains("(exact method)"), "{body}");
    assert!(body.contains("'payment_status'"), "{body}");
    assert!(body.contains("no match: nothingLikeThis"), "{body}");
    // egyetlen string is elég
    let single = s.call(3, "locate", json!({"terms": "Order"}));
    // egyetlen találatnál is a `root:` sor és relatív út
    let one = text(&single).replace('\\', "/");
    assert!(one.starts_with("root: "), "{one}");
    assert!(one.contains("1. src/Order.php"), "{one}");
    let empty = s.call(4, "locate", json!({"terms": []}));
    assert_eq!(empty["isError"], json!(true), "{empty}");
    // a CLI-vel azonos (egy későbbi hívás: az esetleges frissítés-értesítés csak az első eredmény végén áll)
    let again = s.call(5, "locate", args);
    let cli = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args([
            "locate",
            "processRefund",
            "payment_status",
            "nothingLikeThis",
        ])
        .arg("--project")
        .arg(&dir)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .output()
        .unwrap();
    assert_eq!(text(&again), String::from_utf8_lossy(&cli.stdout));
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `project_profile` eszköz: a CLI `profile` kimenete; a `layer` paraméter a `find_callers`-ben szűr.
#[test]
fn project_profile_and_the_layer_parameter() {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("callgraph-mcp-profile-{}-{id}", std::process::id()));
    let files = [
        (
            "composer.json",
            "{\"require\": {\"laravel/framework\": \"^11\"}}\n",
        ),
        (
            "app/Repositories/UserRepository.php",
            "<?php\nnamespace App\\Repositories;\n\nclass UserRepository\n{\n    public function find(int $id): array\n    {\n        return [];\n    }\n}\n",
        ),
        (
            "app/Services/UserService.php",
            "<?php\nnamespace App\\Services;\n\nuse App\\Repositories\\UserRepository;\n\nclass UserService\n{\n    private UserRepository $repo;\n\n    public function show(int $id): array\n    {\n        return $this->repo->find($id);\n    }\n}\n",
        ),
        (
            "app/Http/Controllers/UserController.php",
            "<?php\nnamespace App\\Http\\Controllers;\n\nuse App\\Repositories\\UserRepository;\n\nclass UserController\n{\n    private UserRepository $repo;\n\n    public function show(int $id): array\n    {\n        return $this->repo->find($id);\n    }\n}\n",
        ),
    ];
    for (path, body) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    let dir = dir.canonicalize().unwrap();
    let mut s = Session::start_in(&dir);
    s.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}}),
    );
    let init = s.response(1);
    let instructions = init["result"]["instructions"].as_str().unwrap_or_default();
    assert!(
        instructions.contains("profile: laravel (layered MVC)"),
        "{instructions}"
    );
    s.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let profile = s.call(2, "project_profile", json!({}));
    assert_ne!(profile["isError"], json!(true), "{profile}");
    let body = text(&profile).to_owned();
    assert!(body.starts_with("profile: laravel (layered MVC)"), "{body}");
    assert!(body.contains("repository"), "{body}");
    let cli = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(["profile", "--project"])
        .arg(&dir)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .output()
        .unwrap();
    assert_eq!(body, String::from_utf8_lossy(&cli.stdout));
    let full = s.call(3, "project_profile", json!({"full": true}));
    assert!(text(&full).contains("    app/Services  1"), "{full}");

    let all = s.call(
        4,
        "find_callers",
        json!({"pattern": "UserRepository::find"}),
    );
    assert!(text(&all).contains("UserController"), "{all}");
    assert!(text(&all).contains("UserService"), "{all}");
    let services = s.call(
        5,
        "find_callers",
        json!({"pattern": "UserRepository::find", "layer": "service"}),
    );
    assert!(text(&services).contains("UserService"), "{services}");
    assert!(!text(&services).contains("UserController"), "{services}");
    let wrong = s.call(6, "outline", json!({"path": "app", "layer": "nope"}));
    assert_eq!(wrong["isError"], json!(true));
    assert!(text(&wrong).contains("Unknown layer: nope"), "{wrong}");
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A helyi statisztika `locate`-találata: `locate`, utána `outline` a lista első fájljára → a `callgraph stats`
/// (ugyanazzal a `CALLGRAPH_STATS_DIR`-rel) „1 of 1” első-találatot mutat. Út a napi fájlba nem kerül.
#[test]
fn locate_hit_rank_is_recorded_in_the_local_stats() {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("callgraph-mcp-stats-{}-{id}", std::process::id()));
    let dir = base.join("project");
    let stats = base.join("stats");
    let files = [
        (
            "src/RefundService.php",
            "<?php\nnamespace App;\n\nclass RefundService\n{\n    public function processRefund(int $id): void\n    {\n    }\n}\n",
        ),
        (
            "src/Order.php",
            "<?php\nnamespace App;\n\nclass Order\n{\n    public function paid(): string\n    {\n        return 'refund';\n    }\n}\n",
        ),
    ];
    for (path, body) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    std::fs::create_dir_all(&stats).unwrap();
    let dir = dir.canonicalize().unwrap();
    let mut s = Session::start_with(&dir, &[("CALLGRAPH_STATS_DIR", stats.as_os_str())]);
    s.send(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}}),
    );
    s.response(1);
    s.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let found = s.call(2, "locate", json!({"terms": ["processRefund"]}));
    // egyetlen találatnál az út abszolút (mint a többi lekérdezésnél); a találat-követés mindkét alakot kezeli
    let body = text(&found).replace('\\', "/");
    assert!(
        body.lines()
            .any(|line| line.starts_with("1. ") && line.contains("src/RefundService.php")),
        "{body}"
    );
    let outline = s.call(3, "outline", json!({"path": "src/RefundService.php"}));
    assert_ne!(outline["isError"], json!(true), "{outline}");
    drop(s);

    let report = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .arg("stats")
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_STATS_DIR", &stats)
        .env_remove("CALLGRAPH_STATS")
        .output()
        .unwrap();
    let report = String::from_utf8_lossy(&report.stdout);
    assert!(report.contains(" · 2 calls: mcp 2\n"), "{report}");
    assert!(
        report.contains("first result was the one opened: 1 of 1, top 5: 1 of 1"),
        "{report}"
    );
    // a havi fájl, a munkamenet-állapot és az eseménynapló (`events/`) sem tartalmaz utat vagy nevet
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&stats).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            paths.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
        } else {
            paths.push(path);
        }
    }
    for path in paths {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(
            !text.contains('/') && !text.contains("RefundService"),
            "{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// A `generate` eszköz alapból nincs a listán (a lista változatlan, lásd fent); `CALLGRAPH_GENERATE=1`-gyel megjelenik,
/// íróként annotált, és működik: `dry` kódot ad, `write` új fájlt hoz létre és egy sort ad vissza.
#[test]
fn generate_tool_appears_only_when_enabled() {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("callgraph-mcp-gen-{}-{id}", std::process::id()));
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(
        dir.join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#,
    )
    .unwrap();
    let dir = dir.canonicalize().unwrap();
    let initialize = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "test", "version": "0"}}});
    let list = |s: &mut Session| -> Vec<Value> {
        s.send(initialize.clone());
        s.response(1);
        s.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        s.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
        s.response(2)["result"]["tools"].as_array().unwrap().clone()
    };

    // alapból: nincs `generate` (a meglévő tizenegy eszköz marad)
    let mut off = Session::start_in(&dir);
    let tools = list(&mut off);
    assert_eq!(tools.len(), 11);
    assert!(tools.iter().all(|t| t["name"] != "generate"));
    drop(off);

    let mut on = Session::start_with(&dir, &[("CALLGRAPH_GENERATE", std::ffi::OsStr::new("1"))]);
    let tools = list(&mut on);
    assert_eq!(tools.len(), 12);
    let generate = tools.iter().find(|t| t["name"] == "generate").unwrap();
    let a = &generate["annotations"];
    assert_eq!(
        (
            &a["readOnlyHint"],
            &a["destructiveHint"],
            &a["idempotentHint"],
            &a["openWorldHint"]
        ),
        (&json!(false), &json!(false), &json!(false), &json!(false)),
        "{generate}"
    );
    assert!(
        generate["description"].as_str().unwrap().chars().count() <= 250,
        "{generate}"
    );

    let dry = on.call(
        3,
        "generate",
        json!({"expr": "c:A > +x():void", "dry": true}),
    );
    assert_ne!(dry["isError"], json!(true), "{dry}");
    assert!(text(&dry).contains("class A\n{"), "{dry}");
    let written = on.call(
        4,
        "generate",
        json!({"expr": "c:Pay > +run():int", "write": "app/Pay.php"}),
    );
    assert_ne!(written["isError"], json!(true), "{written}");
    assert!(
        text(&written).starts_with("wrote app/Pay.php ("),
        "{written}"
    );
    assert!(
        std::fs::read_to_string(dir.join("app/Pay.php"))
            .unwrap()
            .contains("namespace App;")
    );
    let again = on.call(
        5,
        "generate",
        json!({"expr": "c:Pay", "write": "app/Pay.php"}),
    );
    assert_eq!(again["isError"], json!(true), "{again}");
    assert!(text(&again).contains("already exists"), "{again}");
    let _ = std::fs::remove_dir_all(&dir);
}
