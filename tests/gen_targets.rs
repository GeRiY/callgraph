//! Kódgenerátor végponttól végpontig, a PHP-n túl: Vue / TS / HTML / `laravel:` preset, és a célzó kapcsolók
//! (`--before`, `--at`, `--upsert`, `--diff`, `::template#id`) a binárison át.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

const COMPOSER: &str =
    r#"{"require":{"tightenco/ziggy":"^2.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#;

const WEB: &str = "<?php\n\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/', function () {\n    return view('welcome');\n});\n";

const MODEL: &str = "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass Fee extends Model\n{\n    protected $fillable = ['name', 'amount'];\n}\n";

const MIGRATION: &str = "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{\n    public function up(): void\n    {\n        Schema::create('fees', function (Blueprint $table) {\n            $table->id();\n            $table->string('name');\n            $table->integer('amount');\n            $table->timestamps();\n        });\n    }\n};\n";

const PAY: &str = "<?php\n\nnamespace App;\n\nclass Pay\n{\n    public function charge(): int\n    {\n        return 1;\n    }\n\n    public function other(): int\n    {\n        return 2;\n    }\n}\n";

/// Egyedi ideiglenes projekt; a végén törlődik.
struct TempProject {
    base: PathBuf,
    root: PathBuf,
}

impl TempProject {
    fn new(files: &[(&str, &str)]) -> Self {
        let base = std::env::temp_dir().join(format!(
            "callgraph-gen-targets-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("project");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(base.join("skill")).unwrap();
        fs::write(root.join("composer.json"), COMPOSER).unwrap();
        for (rel, text) in files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Self {
            root: root.canonicalize().unwrap(),
            base,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.root)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_SKILL_DIR", self.base.join("skill"))
            .current_dir(&self.root)
            .output()
            .expect("callgraph runs")
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).unwrap()
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn vue_write_into_template_by_id_and_undo() {
    let p = TempProject::new(&[("package.json", "{}")]);
    let out = p.run(&[
        "gen",
        "vue:Card>(template>div#main>p{hi})",
        "--write",
        "src/Card.vue",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("wrote src/Card.vue ("),
        "{}",
        stdout(&out)
    );
    let written = p.read("src/Card.vue");
    assert!(written.contains("<div id=\"main\">"), "{written}");

    let out = p.run(&[
        "gen",
        "template>span{more}",
        "--into",
        "src/Card.vue::template#main",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("inserted into src/Card.vue: template L"),
        "{}",
        stdout(&out)
    );
    let inserted = p.read("src/Card.vue");
    assert!(
        inserted.contains("<p>hi</p>\n    <span>more</span>\n  </div>"),
        "{inserted}"
    );

    let out = p.run(&["gen", "template>b", "--into", "src/Card.vue::template#nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("no element matches"),
        "{}",
        stderr(&out)
    );
    assert_eq!(p.read("src/Card.vue"), inserted);

    let out = p.run(&["undo", "src/Card.vue"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(p.read("src/Card.vue"), written);
}

#[test]
fn typescript_class_upsert_and_diff() {
    let svc = "export class Svc {\n  run(): void {\n    this.n++;\n  }\n}\n";
    let p = TempProject::new(&[("src/svc.ts", svc)]);
    let out = p.run(&[
        "gen",
        "+run():void{ this.n = 5 }",
        "--replace",
        "src/svc.ts::Svc::run",
        "--diff",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let shown = stdout(&out);
    assert!(shown.starts_with("→ src/svc.ts replace L2-4"), "{shown}");
    assert!(shown.contains("-     this.n++;"), "{shown}");
    assert!(shown.contains("+     this.n = 5"), "{shown}");
    assert_eq!(p.read("src/svc.ts"), svc);

    let out = p.run(&[
        "gen",
        "+run():void{ this.n = 5 }",
        "--into",
        "src/svc.ts::Svc",
        "--upsert",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("replaced src/svc.ts"),
        "{}",
        stdout(&out)
    );
    assert!(p.read("src/svc.ts").contains("this.n = 5"));
}

#[test]
fn html_write_into_element_and_replace_diff() {
    let p = TempProject::new(&[]);
    let out = p.run(&["gen", "div#box>p{one}", "--write", "page.html"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let out = p.run(&["gen", "p{two}", "--into", "page.html::#box"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let page = p.read("page.html");
    assert!(page.contains("<p>one</p>"), "{page}");
    assert!(page.contains("<p>two</p>"), "{page}");
    assert!(page.find("<p>one</p>") < page.find("<p>two</p>"), "{page}");

    let out = p.run(&[
        "gen",
        "section#box>p{new}",
        "--replace",
        "page.html::#box",
        "--diff",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let shown = stdout(&out);
    assert!(shown.starts_with("→ page.html replace L"), "{shown}");
    assert!(shown.contains("- "), "{shown}");
    assert!(shown.contains("+ "), "{shown}");
    assert_eq!(p.read("page.html"), page);
}

#[test]
fn php_before_at_upsert_and_diff() {
    let p = TempProject::new(&[("app/Pay.php", PAY)]);
    let out = p.run(&[
        "gen",
        "+mid():int{ return 5; }",
        "--into",
        "app/Pay.php",
        "--before",
        "other",
        "--dry",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("→ app/Pay.php::Pay before other (L12)"),
        "{}",
        stdout(&out)
    );
    assert_eq!(p.read("app/Pay.php"), PAY);

    let out = p.run(&[
        "gen",
        "+mid():int{ return 5; }",
        "--into",
        "app/Pay.php",
        "--at",
        "L12",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).contains("Pay at L12 (L12-15)"),
        "{}",
        stdout(&out)
    );
    assert!(p.read("app/Pay.php").contains("public function mid(): int"));

    let out = p.run(&[
        "gen",
        "+other():int{ return 9; }",
        "--into",
        "app/Pay.php",
        "--upsert",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stdout(&out).starts_with("replaced app/Pay.php L"),
        "{}",
        stdout(&out)
    );
    assert!(p.read("app/Pay.php").contains("return 9;"));

    let out = p.run(&[
        "gen",
        "+charge():int{ return 7; }",
        "--replace",
        "app/Pay.php::charge",
        "--diff",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let shown = stdout(&out);
    assert!(shown.contains("-         return 1;"), "{shown}");
    assert!(shown.contains("+         return 7;"), "{shown}");

    let out = p.run(&[
        "gen",
        "+x()",
        "--into",
        "app/Pay.php",
        "--after",
        "a",
        "--before",
        "b",
    ]);
    assert!(!out.status.success());
}

#[test]
fn laravel_preset_dry_names_the_files_it_would_write() {
    let p = TempProject::new(&[
        ("routes/web.php", WEB),
        ("app/Models/Fee.php", MODEL),
        (
            "database/migrations/2024_01_01_000000_create_fees_table.php",
            MIGRATION,
        ),
    ]);
    let out = p.run(&["gen", "laravel:Fee>create(name:str,amount:int)", "--dry"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let shown = stdout(&out);
    assert!(!shown.trim().is_empty(), "{shown}");
    // --dry: semmi nem íródik
    assert!(
        !p.root
            .join("app/Http/Controllers/FeeController.php")
            .exists()
    );

    let out = p.run(&["gen", "laravel:Fee>create(name:str,amount:int)"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        p.root
            .join("app/Http/Controllers/FeeController.php")
            .exists()
    );
    assert!(p.root.join("resources/js/Pages/Fees/Create.vue").exists());
}
