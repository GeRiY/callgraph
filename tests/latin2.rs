//! Nem UTF-8 (pl. ISO-8859-2) forrásfájl: a `--lines` és a `--body` a sorokat írja ki, nem üres kimenetet.

use std::path::PathBuf;
use std::process::Command;

fn project() -> PathBuf {
    let root = std::env::temp_dir().join(format!("callgraph-it-latin2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("ExampleController.php"),
        b"<?php\nclass ExampleController\n{\n    public function actionIndex()\n    {\n        // t\xf6bb v\xe1ltoz\xf3\n        $x = Foo::bar();\n    }\n}\n",
    )
    .unwrap();
    root
}

fn show(root: &PathBuf, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .arg("show")
        .args(args)
        .arg("--project")
        .arg(root)
        .current_dir(root)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_UPDATE_CHECK", "0")
        .env("CALLGRAPH_STATS", "0")
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn lines_and_body_of_a_latin2_file_are_not_empty() {
    let root = project();
    let lines = show(&root, &["ExampleController.php", "--lines", "1-9"]);
    assert!(lines.contains("$x = Foo::bar();"), "{lines}");
    assert!(lines.contains("public function actionIndex()"), "{lines}");
    let body = show(&root, &["ExampleController.php::actionIndex", "--body"]);
    assert!(body.contains("$x = Foo::bar();"), "{body}");
    let _ = std::fs::remove_dir_all(root);
}
