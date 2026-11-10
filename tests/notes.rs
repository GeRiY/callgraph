//! Jegyzetek (pilot): `callgraph note add|list|rm` és a megjelenítésük (`show` outline, `--body`, `--notes`)
//! a binárison át, egy ideiglenes projekten.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

const SHOP: &str = "<?php\nclass Shop\n{\n    public function run(): int\n    {\n        return strlen('a');\n    }\n\n    public function other(): int\n    {\n        return 2;\n    }\n}\n";
const SHOP_WITHOUT_RUN: &str =
    "<?php\nclass Shop\n{\n    public function other(): int\n    {\n        return 2;\n    }\n}\n";

/// Egyedi ideiglenes projekt (folyamat-id + számláló); a végén törlődik.
struct TempProject {
    root: PathBuf,
    skill: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "callgraph-notes-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("project");
        let skill = base.join("skill");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(root.join("src/shop.php"), SHOP).unwrap();
        Self {
            root: root.canonicalize().unwrap(),
            skill,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_SKILL_DIR", &self.skill)
            .current_dir(&self.root)
            .output()
            .expect("callgraph runs")
    }

    fn root(&self) -> &str {
        self.root.to_str().unwrap()
    }

    /// `show --project <gyökér> <args>` stdout-ja (sikeres futás).
    fn show(&self, args: &[&str]) -> String {
        let mut all = vec!["show", "--project", self.root()];
        all.extend_from_slice(args);
        let out = self.run(&all);
        assert!(out.status.success(), "{}", stderr(&out));
        stdout(&out)
    }

    fn note(&self, args: &[&str]) -> Output {
        let mut all = vec!["note"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["--project", self.root()]);
        self.run(&all)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        if let Some(base) = self.root.parent() {
            let _ = fs::remove_dir_all(base);
        }
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Az első szó (az új jegyzet azonosítója) a `note add` kimenetéből.
fn added_id(out: &Output) -> String {
    assert!(out.status.success(), "{}", stderr(out));
    let text = stdout(out);
    let id = text.split_whitespace().next().unwrap().to_owned();
    assert_eq!(id.len(), 8, "{text}");
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "{text}");
    id
}

fn time_re() -> regex::Regex {
    regex::Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}").unwrap()
}

/// Fájl újraírása későbbi mtime-mal (a cache másodperc-pontos mtime-ot néz, így az új tartalom biztosan látszik).
fn rewrite(path: &Path, text: &str, ahead_secs: u64) {
    fs::write(path, text).unwrap();
    let when = std::time::SystemTime::now() + std::time::Duration::from_secs(ahead_secs);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(when)
        .unwrap();
}

/// A `pattern`-t tartalmazó első sor indexe.
fn line_of(text: &str, pattern: &str) -> usize {
    // Windowson az utak `\`-sel jönnek; a mintában egységesen `/`
    text.lines()
        .position(|l| l.replace('\\', "/").contains(pattern))
        .unwrap_or_else(|| panic!("no line with {pattern:?}:\n{text}"))
}

#[test]
fn notes_round_trip_through_the_binary() {
    let p = TempProject::new();
    let file = "src/shop.php";
    let outline_before = p.show(&[file]);
    let json_before = p.show(&["--format", "json", file]);
    let body_before = p.show(&["--body", "src/shop.php::run"]);

    let method_id = added_id(&p.note(&["add", "src/shop.php::run", "Slow: N+1 query"]));
    let file_id = added_id(&p.note(&["add", file, "Legacy file, do not extend"]));
    // a horgony a konténert is rögzíti
    let list = stdout(&p.note(&["list"]));
    assert!(
        list.contains(&format!("{method_id}  src/shop.php::Shop::run  ")),
        "{list}"
    );
    assert!(
        list.contains(&format!("{file_id}  src/shop.php  ")),
        "{list}"
    );
    assert!(time_re().is_match(&list), "{list}");
    assert!(!list.contains("[orphaned]"), "{list}");
    // a tároló: <gyökér>/.callgraph/notes.jsonl, a .callgraph/ *-os .gitignore-ral
    let stored = fs::read_to_string(p.root.join(".callgraph/notes.jsonl")).unwrap();
    assert_eq!(stored.lines().count(), 2, "{stored}");
    assert!(p.root.join(".callgraph/.gitignore").is_file());

    // nem létező metódus / fájl: hiba, nincs mentés
    let missing = p.note(&["add", "src/shop.php::nope", "x"]);
    assert!(!missing.status.success());
    assert!(
        stderr(&missing).contains("No such method"),
        "{}",
        stderr(&missing)
    );
    assert!(!p.note(&["add", "src/none.php", "x"]).status.success());

    // outline: a fájl-jegyzet a FILE fejléc alatt, a metódusé a metódus-sor alatt, idővel
    let outline = p.show(&[file]);
    let header = line_of(&outline, "FILE  ");
    let file_note = line_of(&outline, "· Legacy file, do not extend");
    assert_eq!(file_note, header + 1, "{outline}");
    assert!(outline.lines().nth(file_note).unwrap().starts_with("  📝 "));
    let run = line_of(&outline, "  run()  L");
    let method_note = line_of(&outline, "· Slow: N+1 query");
    assert_eq!(method_note, run + 1, "{outline}");
    let line = outline.lines().nth(method_note).unwrap();
    assert!(line.starts_with("    📝 "), "{line}");
    assert!(time_re().is_match(line), "{line}");
    assert!(
        !outline[outline.find("  other()").unwrap()..].contains("📝"),
        "{outline}"
    );
    // a gépi formátum változatlan
    assert_eq!(p.show(&["--format", "json", file]), json_before);
    // --body: a jegyzet a fejléc után, a forrás előtt
    let body = p.show(&["--body", "src/shop.php::run"]);
    assert_eq!(
        line_of(&body, "· Slow: N+1 query"),
        line_of(&body, "== ") + 1,
        "{body}"
    );

    // --notes: keresés a mintára (kis- és nagybetű mindegy), a horgony és az idő látszik
    let found = p.show(&["src", "--notes", "n\\+1"]);
    assert_eq!(found.lines().count(), 1, "{found}");
    // az út a platform elválasztójával jön (Windowson `src\shop.php`)
    assert!(
        found
            .replace('\\', "/")
            .contains("src/shop.php::Shop::run  "),
        "{found}"
    );
    assert!(found.contains("  📝 Slow: N+1 query"), "{found}");
    assert!(time_re().is_match(&found), "{found}");
    let all = p.show(&[".", "--notes", ""]);
    assert!(all.starts_with("root: "), "{all}");
    assert_eq!(all.lines().count(), 3, "{all}");
    let none = p.show(&["src", "--notes", "zzz"]);
    assert_eq!(none, "(no matching notes)\n");

    // a metódus eltűnik → árva: a lista jelöli, az outline nem mutatja
    rewrite(&p.root.join(file), SHOP_WITHOUT_RUN, 10);
    let list = stdout(&p.note(&["list", "src"]));
    let orphan = list.lines().find(|l| l.starts_with(&method_id)).unwrap();
    assert!(orphan.ends_with("[orphaned]"), "{list}");
    let outline = p.show(&[file]);
    assert!(!outline.contains("Slow"), "{outline}");
    assert!(outline.contains("Legacy file"), "{outline}");
    rewrite(&p.root.join(file), SHOP, 20);

    // törlés: utána a kimenet byte-ra azonos a jegyzet nélkülivel
    assert!(p.note(&["rm", &method_id]).status.success());
    assert!(p.note(&["rm", &file_id]).status.success());
    assert!(!p.note(&["rm", &file_id]).status.success());
    assert_eq!(stdout(&p.note(&["list"])), "(no notes)\n");
    assert_eq!(p.show(&[file]), outline_before);
    assert_eq!(p.show(&["--body", "src/shop.php::run"]), body_before);
}

#[test]
fn note_without_project_uses_the_working_directory() {
    let p = TempProject::new();
    let id = added_id(&p.run(&["note", "add", "src/shop.php::other", "returns", "two"]));
    let list = stdout(&p.run(&["note", "list", Path::new("src").to_str().unwrap()]));
    assert!(
        list.contains(&format!("{id}  src/shop.php::Shop::other  ")),
        "{list}"
    );
    assert!(list.trim_end().ends_with("📝 returns two"), "{list}");
}

const CART: &str = "<?php\nclass Cart\n{\n    public function total(): int\n    {\n        return (new Shop())->run();\n    }\n}\n";
const CHECKOUT: &str = "<?php\nclass Checkout\n{\n    public function pay(): int\n    {\n        return (new Cart())->total();\n    }\n}\n";

/// A `pattern`-t tartalmazó első sor.
fn line_with<'a>(text: &'a str, pattern: &str) -> &'a str {
    text.lines().nth(line_of(text, pattern)).unwrap()
}

#[test]
fn note_marker_on_callers_locate_and_api() {
    let p = TempProject::new();
    fs::write(p.root.join("src/cart.php"), CART).unwrap();
    fs::write(p.root.join("src/checkout.php"), CHECKOUT).unwrap();
    let grep = ["--grep", "run$", "src"];
    let flat = ["CALLGRAPH_OUT_LIMIT=4", "--grep", "run$", "src"];
    let deep = ["--grep", "run$", "--depth", "2", "--guess", "src"];
    let api = ["--format", "api", "src/cart.php"];
    let locate = || stdout(&p.run(&["locate", "Cart", "Shop", "--project", p.root()]));
    // jegyzet nélkül nincs jelölő
    for output in [
        p.show(&grep),
        p.show(&flat),
        p.show(&deep),
        p.show(&api),
        locate(),
    ] {
        assert!(!output.contains("📝"), "{output}");
    }

    added_id(&p.note(&["add", "src/cart.php::total", "Caller remark"]));
    added_id(&p.note(&["add", "src/checkout.php", "Payment file"]));
    added_id(&p.note(&["add", "src/shop.php::run", "Hot path"]));
    added_id(&p.note(&["add", "src/shop.php", "Shop file"]));

    // --grep outline: a hívó metódus-során a metódus jegyzetei; a FILE sor fájl-jegyzet nélkül jelöletlen
    let out = p.show(&grep);
    assert!(line_with(&out, "total()  L").ends_with("  📝 1"), "{out}");
    assert!(!line_with(&out, "FILE  ").contains("📝"), "{out}");
    // hívásonként egy sor (kompakt nézet)
    let out = p.show(&flat);
    assert!(
        line_with(&out, "Cart::total").contains("Cart::total  📝 1  L"),
        "{out}"
    );
    // --depth 2: az 1. szinten a metódus-jegyzet, a 2. szinten (csak fájl-jegyzet) a fájlé
    let out = p.show(&deep);
    assert!(line_with(&out, "Cart::total()").contains("  📝 1"), "{out}");
    assert!(
        line_with(&out, "Checkout::pay()").contains("  📝 1  ← "),
        "{out}"
    );
    // api: a metódus-sor végén
    let out = p.show(&api);
    assert!(line_with(&out, "total(").ends_with("  📝 1"), "{out}");
    let out = p.show(&["--format", "api", "src/checkout.php"]);
    assert!(line_with(&out, "FILE  ").ends_with("  📝 1"), "{out}");
    // locate: a fájl-sor végén a fájl összes (nem árva) jegyzete
    let out = locate();
    assert!(
        line_with(&out, ". src/shop.php  ").ends_with("  📝 2"),
        "{out}"
    );
    assert!(
        line_with(&out, ". src/cart.php  ").ends_with("  📝 1"),
        "{out}"
    );
    rewrite(&p.root.join("src/shop.php"), SHOP_WITHOUT_RUN, 5);
    let out = locate();
    assert!(
        line_with(&out, ". src/shop.php  ").ends_with("  📝 1"),
        "{out}"
    );
}
