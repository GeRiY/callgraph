//! A generátor egységtesztjei: tokenizáló / parser, PHP-emitter, a feloldó-lánc, a szerkesztő-parancsok (memóriabeli
//! fájlrendszerrel és rögzített órával), a mentések és az undo.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::ast::TypeKind;
use super::backup::Backups;
use super::fs::{Clock, DirEntry, Env, FileSystem, stamp, walk_php};
use super::generator::{GenRequest, Generator, Output, ReplaceAt, Target};
use super::php::PhpEmitter;
use super::project::Psr4;
use super::{Emitter, parser};

/// Memóriabeli fájlrendszer (abszolút utak, `/p` a gyökér).
#[derive(Default)]
struct MemFs {
    files: RefCell<BTreeMap<PathBuf, String>>,
}

impl MemFs {
    fn with(files: &[(&str, &str)]) -> Self {
        let fs = Self::default();
        for (path, text) in files {
            fs.files
                .borrow_mut()
                .insert(Path::new(ROOT).join(path), (*text).to_owned());
        }
        fs
    }

    fn get(&self, rel: &str) -> Option<String> {
        self.files.borrow().get(&Path::new(ROOT).join(rel)).cloned()
    }

    fn names_under(&self, rel: &str) -> Vec<String> {
        let dir = Path::new(ROOT).join(rel);
        self.files
            .borrow()
            .keys()
            .filter(|p| p.starts_with(&dir))
            .map(|p| p.display().to_string())
            .collect()
    }
}

impl FileSystem for MemFs {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, path: &Path, content: &str) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_owned());
        Ok(())
    }

    fn create_new(&self, path: &Path, content: &str) -> io::Result<()> {
        if self.files.borrow().contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        self.write(path, content)
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.borrow().contains_key(path)
    }

    fn list(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        let mut entries: BTreeMap<PathBuf, bool> = BTreeMap::new();
        for path in self.files.borrow().keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let is_dir = parts.next().is_some();
            entries.insert(dir.join(first.as_os_str()), is_dir);
        }
        if entries.is_empty() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(entries
            .into_iter()
            .map(|(path, is_dir)| DirEntry { path, is_dir })
            .collect())
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        self.files.borrow_mut().remove(path);
        Ok(())
    }
}

const ROOT: &str = "/p";

/// Rögzített, léptethető óra.
struct FixedClock(Cell<u64>);

impl Clock for FixedClock {
    fn now_secs(&self) -> u64 {
        self.0.get()
    }
}

/// 2023-11-14T22:13:20Z
const T0: u64 = 1_700_000_000;

fn env<'a>(fs: &'a MemFs, clock: &'a FixedClock) -> Env<'a> {
    Env {
        root: Path::new(ROOT),
        fs,
        clock,
    }
}

fn request(expr: &str, dry: bool, target: Target) -> GenRequest {
    GenRequest {
        expr: expr.to_owned(),
        dry,
        target,
    }
}

fn run(fs: &MemFs, clock: &FixedClock, req: &GenRequest) -> Result<Output, String> {
    Generator::new(env(fs, clock), PhpEmitter)
        .run(req)
        .map_err(|e| e.to_string())
}

/// A deklarációk sorai (a névtér és az importok nélkül).
fn emit(expr: &str) -> String {
    let program = parser::parse(expr).unwrap();
    let emitter = PhpEmitter;
    let mut lines = Vec::new();
    for decl in &program.decls {
        lines.extend(emitter.declaration(decl));
    }
    lines.join("\n")
}

/// A csupasz tagok sorai.
fn emit_members(expr: &str) -> String {
    let program = parser::parse(expr).unwrap();
    PhpEmitter
        .members(TypeKind::Class, &program.loose)
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_err(expr: &str) -> String {
    parser::parse(expr).unwrap_err().to_string()
}

// ---- tokenizáló / parser ----

#[test]
fn parser_builds_the_tree_with_child_sibling_and_climb() {
    let p = parser::parse("c:A > +x():int + +y():int ^ c:B > +z():int").unwrap();
    assert_eq!(p.decls.len(), 2);
    assert_eq!(p.decls[0].name, "A");
    assert_eq!(p.decls[0].members.len(), 2);
    assert_eq!(p.decls[1].name, "B");
    assert_eq!(p.decls[1].members[0].name, "z");
}

#[test]
fn parser_treats_a_new_line_as_a_sibling() {
    let p = parser::parse("c:A >\n  +x():int\n  +y():int\n  -$z:int").unwrap();
    assert_eq!(p.decls[0].members.len(), 3);
    assert_eq!(p.decls[0].members[2].name, "z");
}

#[test]
fn parser_expands_groups_repeat_and_counters() {
    let p = parser::parse("c:A > (+get$():int + +set$(int $v):void)*2").unwrap();
    let names: Vec<&str> = p.decls[0].members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["get1", "set1", "get2", "set2"]);
    let p = parser::parse("c:Item$$*3").unwrap();
    let names: Vec<&str> = p.decls.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["Item01", "Item02", "Item03"]);
    let p = parser::parse("c:A > +m$():int*2").unwrap();
    assert_eq!(p.decls[0].members.len(), 2);
}

#[test]
fn parser_keeps_raw_bodies_with_nested_braces_and_quotes() {
    let p = parser::parse(
        "c:A > +f():int{ if ($x) { return '}'; } // it's }\n return 2; } + +g():void{}",
    )
    .unwrap();
    let f = &p.decls[0].members[0];
    assert_eq!(
        f.body.as_deref(),
        Some("if ($x) { return '}'; } // it's }\nreturn 2;")
    );
    assert_eq!(p.decls[0].members[1].body.as_deref(), Some(""));
}

#[test]
fn parser_accepts_spaces_around_colon_and_equals() {
    let p = parser::parse("c:A > +f(int $a) : ?int { return 1; } + -$x : int = 5").unwrap();
    assert_eq!(p.decls[0].members[0].name, "f");
    assert_eq!(p.decls[0].members[1].default.as_deref(), Some("5"));
}

#[test]
fn parser_reports_error_positions() {
    let e = parse_err("c:A > > +x()");
    assert!(e.contains("col 7"), "{e}");
    let e = parse_err("c:A > +x(){ return 1;");
    assert!(e.contains("unclosed"), "{e}");
    let e = parse_err("c:A x()");
    assert!(e.contains("col 5"), "{e}");
    let e = parse_err("c:A > +x(int $a");
    assert!(e.contains("unclosed"), "{e}");
    assert!(parse_err("").contains("empty"));
    assert!(parse_err("c:A ^ c:B").contains("`^`"));
    assert!(parse_err("c:A > frobnicate").contains("unknown element"));
    assert!(parse_err("c:A > +x()*0").contains("count"));
    assert!(parse_err("(c:A) > +x()").contains("group"));
    assert!(parse_err("c:A)").contains("`)`"));
}

#[test]
fn parser_rejects_mixed_top_level_and_directive_only() {
    assert!(parse_err("c:A + +x()").contains("cannot be mixed"));
    assert!(parse_err("ns:App\\X").contains("nothing to generate"));
}

#[test]
fn parser_reads_namespace_and_explicit_uses() {
    let p = parser::parse("ns:App\\Svc + use:Foo\\Bar,Baz\\Qux as Q + c:A").unwrap();
    assert_eq!(p.namespace.as_deref(), Some("App\\Svc"));
    assert_eq!(p.uses.len(), 2);
    assert_eq!(p.uses[1].fqcn, "Baz\\Qux");
    assert_eq!(p.uses[1].alias.as_deref(), Some("Q"));
}

#[test]
fn neutral_types_parse_and_native_names_stay() {
    use super::ast::TypeRef;
    assert_eq!(TypeRef::parse("str").unwrap(), TypeRef::Str);
    assert_eq!(
        TypeRef::parse("map<str,list<int>>").unwrap(),
        TypeRef::Map(
            Box::new(TypeRef::Str),
            Box::new(TypeRef::List(Box::new(TypeRef::Int)))
        )
    );
    assert_eq!(
        TypeRef::parse("?Order").unwrap(),
        TypeRef::Opt(Box::new(TypeRef::Named("Order".into())))
    );
    assert_eq!(
        TypeRef::parse("opt<int>").unwrap(),
        TypeRef::Opt(Box::new(TypeRef::Int))
    );
    assert_eq!(TypeRef::parse("string").unwrap(), TypeRef::Named("string".into()));
    assert!(matches!(TypeRef::parse("int|string").unwrap(), TypeRef::Union(_)));
    assert!(TypeRef::parse("Collection<Order>").is_err());
}

// ---- PHP-emitter ----

#[test]
fn emits_a_final_class_with_extends_and_implements() {
    assert_eq!(
        emit("c!:Pay<Base>[Chargeable,Refundable] > +run():void"),
        "final class Pay extends Base implements Chargeable, Refundable\n{\n    public function run(): void\n    {\n        // TODO\n    }\n}"
    );
}

#[test]
fn emits_backed_and_pure_enums() {
    assert_eq!(
        emit("e:Status:string(Draft=draft,Paid=paid)"),
        "enum Status: string\n{\n    case Draft = 'draft';\n    case Paid = 'paid';\n}"
    );
    assert_eq!(
        emit("e:Level:int(Low=1,High=2)"),
        "enum Level: int\n{\n    case Low = 1;\n    case High = 2;\n}"
    );
    assert_eq!(
        emit("e:Dir(North,South)"),
        "enum Dir\n{\n    case North;\n    case South;\n}"
    );
}

#[test]
fn emits_interfaces_and_traits() {
    assert_eq!(
        emit("i:Chargeable<Base> > +charge(int $amount):bool + +K=1"),
        "interface Chargeable extends Base\n{\n    public function charge(int $amount): bool;\n\n    public const K = 1;\n}"
    );
    assert_eq!(
        emit("t:HasName > -$name:str='x' + #$count:int + +K:str='a'"),
        "trait HasName\n{\n    private string $name = 'x';\n\n    protected int $count;\n\n    public const string K = 'a';\n}"
    );
}

#[test]
fn emits_promoted_ctor_static_abstract_and_nullable_members() {
    let text = emit(
        "c~:Repo > ctor(-Db $db, +readonly int $n=1) + +::make(): static{ return new static(); } + #~find(int $id):?Model + -$cache:?Model",
    );
    assert_eq!(
        text,
        "abstract class Repo\n{\n    public function __construct(\n        private Db $db,\n        public readonly int $n = 1,\n    ) {}\n\n    public static function make(): static\n    {\n        return new static();\n    }\n\n    abstract protected function find(int $id): ?Model;\n\n    private ?Model $cache = null;\n}"
    );
}

#[test]
fn todo_bodies_follow_one_rule() {
    let text = emit(
        "c:Svc > +a(?int $x=null, string ...$rest):int + +b():Generator + +e():void + +n() + ctor(int $a)",
    );
    assert!(text.contains("public function a(?int $x = null, string ...$rest): int\n    {\n        throw new \\LogicException('TODO');"), "{text}");
    assert!(text.contains("public function b(): Generator\n    {\n        yield from [];"), "{text}");
    assert!(text.contains("public function e(): void\n    {\n        // TODO"), "{text}");
    assert!(text.contains("public function n()\n    {\n        // TODO"), "{text}");
    assert!(text.contains("public function __construct(int $a)\n    {\n        // TODO"), "{text}");
}

#[test]
fn neutral_types_map_to_php_with_docblocks() {
    let text = emit(
        "c:Svc > +c(): iter<int> + +d(list<Order> $orders, map<str,int> $m, opt<int> $o):list<Order> + +s(str $a, bytes $b, float $f, bool $c):mixed",
    );
    assert!(text.contains("     * @return iterable<int>\n     */\n    public function c(): iterable\n    {\n        yield from [];"), "{text}");
    assert!(text.contains("@param list<Order> $orders\n     * @param array<string,int> $m\n     * @return list<Order>\n     */\n    public function d(array $orders, array $m, ?int $o): array"), "{text}");
    assert!(text.contains("public function s(string $a, string $b, float $f, bool $c): mixed"), "{text}");
}

#[test]
fn list_properties_get_a_var_docblock() {
    let text = emit("c:A > -$items:list<Order>=[]");
    assert!(text.contains("    /** @var list<Order> */\n    private array $items = [];"), "{text}");
}

#[test]
fn long_parameter_lists_wrap_with_trailing_commas() {
    let text = emit(
        "c:A > +handle(string $firstArgument, string $secondArgument, string $thirdArgument, string $fourthArgument):void",
    );
    assert!(text.contains("    public function handle(\n        string $firstArgument,\n"), "{text}");
    assert!(text.contains("        string $fourthArgument,\n    ): void {\n        // TODO\n    }"), "{text}");
}

#[test]
fn bare_members_emit_at_class_indent() {
    assert_eq!(
        emit_members("+a():void + +b():int{ return 1; }"),
        "    public function a(): void\n    {\n        // TODO\n    }\n\n    public function b(): int\n    {\n        return 1;\n    }"
    );
}

#[test]
fn generator_bodies_pass_through_and_validate() {
    let fs = MemFs::default();
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &request(
            "+items():iter<int>{ yield 1; yield from [2, 3]; } + +none():Generator",
            true,
            Target::Preview,
        ),
    )
    .unwrap();
    let text = out.lines.join("\n");
    assert!(text.contains("yield 1;"), "{text}");
    assert!(text.contains("yield from [];"), "{text}");
}

// ---- projekt-ismeret, feloldó-lánc ----

const COMPOSER: &str = r#"{"autoload":{"psr-4":{"App\\":"app/"}},"autoload-dev":{"psr-4":{"Tests\\":["tests/"]}}}"#;

fn project() -> MemFs {
    MemFs::with(&[
        ("composer.json", COMPOSER),
        (
            "app/Models/Order.php",
            "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Models;\n\nclass Order {}\n",
        ),
        (
            "app/Dto/Receipt.php",
            "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Dto;\n\nclass Receipt {}\n",
        ),
        (
            "app/Other.php",
            "<?php\n\ndeclare(strict_types=1);\n\nnamespace App;\n\nuse GuzzleHttp\\Client;\n\nclass Other {}\n",
        ),
        (
            "app/Services/Pay.php",
            "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Services;\n\nuse App\\Models\\Order;\n\nclass Pay\n{\n    public function charge(Order $o): void\n    {\n    }\n\n    public function other(): int\n    {\n        return 1;\n    }\n}\n",
        ),
    ])
}

#[test]
fn psr4_maps_paths_to_namespaces() {
    let map = Psr4::parse(COMPOSER);
    assert_eq!(map.namespace_of("app/Services/Pay.php").as_deref(), Some("App\\Services"));
    assert_eq!(map.namespace_of("app/Pay.php").as_deref(), Some("App"));
    assert_eq!(map.fqcn_of("app/Models/Order.php").as_deref(), Some("App\\Models\\Order"));
    assert_eq!(map.namespace_of("tests/Unit/PayTest.php").as_deref(), Some("Tests\\Unit"));
    assert_eq!(map.namespace_of("lib/X.php"), None);
    assert!(Psr4::parse("not json").is_empty());
}

#[test]
fn walk_is_lazy_and_skips_vendor() {
    let fs = MemFs::with(&[
        ("app/A.php", ""),
        ("app/vendor/B.php", ""),
        ("app/Z/C.php", ""),
        ("app/readme.md", ""),
    ]);
    let found: Vec<String> = walk_php(&fs, Path::new(ROOT).join("app"))
        .map(|p| p.display().to_string())
        .collect();
    assert_eq!(found, ["/p/app/A.php", "/p/app/Z/C.php"]);
    let first = walk_php(&fs, Path::new(ROOT).join("app")).next();
    assert_eq!(first, Some(PathBuf::from("/p/app/A.php")));
}

#[test]
fn new_file_gets_namespace_sorted_imports_and_strict_types() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &request(
            "c:Pay2 > ctor(-Client $c) + +charge(Order $o):Receipt + +fail():void{ throw new DomainException(); }",
            true,
            Target::Write {
                file: "app/Services/Pay2.php".into(),
            },
        ),
    )
    .unwrap();
    let text = out.lines.join("\n");
    assert_eq!(
        text,
        "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Services;\n\nuse App\\Dto\\Receipt;\nuse App\\Models\\Order;\nuse DomainException;\nuse GuzzleHttp\\Client;\n\nclass Pay2\n{\n    public function __construct(\n        private Client $c,\n    ) {}\n\n    public function charge(Order $o): Receipt\n    {\n        throw new \\LogicException('TODO');\n    }\n\n    public function fail(): void\n    {\n        throw new DomainException();\n    }\n}"
    );
    assert!(out.unresolved.is_empty(), "{:?}", out.unresolved);
}

#[test]
fn unknown_names_are_reported_not_imported() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &request("c:A > +f(Mystery $m, Order $o):void", true, Target::Preview),
    )
    .unwrap();
    assert_eq!(out.unresolved, ["Mystery"]);
    assert!(out.render().ends_with("unresolved: Mystery\n"));
}

#[test]
fn explicit_use_and_namespace_override_win() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &request(
            "ns:Acme\\Pay + use:Vendor\\Lib\\Order as Ord + c:A > +f(Ord $a, Order $b):void",
            true,
            Target::Write {
                file: "app/A.php".into(),
            },
        ),
    )
    .unwrap();
    let text = out.lines.join("\n");
    assert!(text.contains("namespace Acme\\Pay;"), "{text}");
    assert!(text.contains("use App\\Models\\Order;\nuse Vendor\\Lib\\Order as Ord;"), "{text}");
}

#[test]
fn strict_types_follows_the_project_majority() {
    let fs = MemFs::with(&[
        ("composer.json", COMPOSER),
        ("app/A.php", "<?php\nclass A {}\n"),
        ("app/B.php", "<?php\nclass B {}\n"),
        ("app/C.php", "<?php\ndeclare(strict_types=1);\nclass C {}\n"),
    ]);
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &request(
            "c:D",
            true,
            Target::Write {
                file: "app/D.php".into(),
            },
        ),
    )
    .unwrap();
    assert!(!out.lines.join("\n").contains("strict_types"));
}

// ---- írók ----

#[test]
fn write_creates_a_new_file_and_refuses_an_existing_one() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let req = request(
        "c:Fresh > +run():void",
        false,
        Target::Write {
            file: "app/Fresh.php".into(),
        },
    );
    let out = run(&fs, &clock, &req).unwrap();
    let text = fs.get("app/Fresh.php").unwrap();
    assert!(text.contains("namespace App;\n\nclass Fresh"), "{text}");
    assert_eq!(
        out.lines,
        [format!(
            "wrote app/Fresh.php ({} lines) · backup: none",
            text.lines().count()
        )]
    );
    let err = run(&fs, &clock, &req).unwrap_err();
    assert!(err.contains("already exists") && err.contains("--into"), "{err}");
    assert_eq!(fs.get("app/Fresh.php").unwrap(), text);
}

#[test]
fn write_rejects_members_only_wrong_extension_and_outside_paths() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let write = |expr: &str, file: &str| {
        run(
            &fs,
            &clock,
            &request(expr, false, Target::Write { file: file.into() }),
        )
        .unwrap_err()
    };
    assert!(write("+x():void", "app/X.php").contains("needs a type"));
    assert!(write("c:X", "app/X.txt").contains(".php"));
    assert!(write("c:X", "../X.php").contains("outside"));
    assert!(write("c:X", "/etc/X.php").contains("outside"));
}

#[test]
fn into_inserts_after_a_method_and_merges_imports() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &request(
            "+refund(Receipt $r):void{ $this->x = new Order(); }",
            false,
            Target::Into {
                file: "app/Services/Pay.php".into(),
                class: Some("Pay".into()),
                after: Some("charge".into()),
            },
        ),
    )
    .unwrap();
    let text = fs.get("app/Services/Pay.php").unwrap();
    assert_eq!(
        text,
        "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Services;\n\nuse App\\Dto\\Receipt;\nuse App\\Models\\Order;\n\nclass Pay\n{\n    public function charge(Order $o): void\n    {\n    }\n\n    public function refund(Receipt $r): void\n    {\n        $this->x = new Order();\n    }\n\n    public function other(): int\n    {\n        return 1;\n    }\n}\n"
    );
    let backup = format!(".callgraph/backups/app/Services/Pay.php.{}.bak", stamp(T0));
    assert_eq!(
        out.lines,
        [format!(
            "inserted 1 member(s) into app/Services/Pay.php::Pay after charge (L16-19) · backup: {backup}"
        )]
    );
    assert!(fs.get(&backup).unwrap().contains("public function other"));
}

#[test]
fn into_appends_at_the_end_when_no_method_is_given() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    run(
        &fs,
        &clock,
        &request(
            "+last():int{ return 3; }",
            false,
            Target::Into {
                file: "app/Services/Pay.php".into(),
                class: None,
                after: None,
            },
        ),
    )
    .unwrap();
    let text = fs.get("app/Services/Pay.php").unwrap();
    assert!(
        text.ends_with("        return 1;\n    }\n\n    public function last(): int\n    {\n        return 3;\n    }\n}\n"),
        "{text}"
    );
}

#[test]
fn into_an_empty_class_has_no_leading_blank_line() {
    let fs = MemFs::with(&[("app/E.php", "<?php\n\nclass E\n{\n}\n")]);
    let clock = FixedClock(Cell::new(T0));
    run(
        &fs,
        &clock,
        &request(
            "+a():void",
            false,
            Target::Into {
                file: "app/E.php".into(),
                class: None,
                after: None,
            },
        ),
    )
    .unwrap();
    assert_eq!(
        fs.get("app/E.php").unwrap(),
        "<?php\n\nclass E\n{\n    public function a(): void\n    {\n        // TODO\n    }\n}\n"
    );
}

#[test]
fn into_reports_missing_class_and_method() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let into = |class: Option<&str>, after: Option<&str>| {
        run(
            &fs,
            &clock,
            &request(
                "+x():void",
                false,
                Target::Into {
                    file: "app/Services/Pay.php".into(),
                    class: class.map(str::to_owned),
                    after: after.map(str::to_owned),
                },
            ),
        )
        .unwrap_err()
    };
    assert!(into(Some("Nope"), None).contains("no class `Nope`"));
    let e = into(None, Some("nope"));
    assert!(e.contains("no method `nope`") && e.contains("charge, other"), "{e}");
}

const DOCUMENTED: &str = "<?php\n\nclass A\n{\n    /**\n     * Old doc.\n     */\n    public function go(): int\n    {\n        return 1;\n    }\n\n    public function keep(): int\n    {\n        return 2;\n    }\n}\n";

#[test]
fn replace_by_method_swaps_the_docblock_too() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &request(
            "+go(int $x):list<int>{ return [$x]; }",
            false,
            Target::Replace {
                file: "app/A.php".into(),
                at: ReplaceAt::Method {
                    class: None,
                    name: "go".into(),
                },
                expect: None,
            },
        ),
    )
    .unwrap();
    assert_eq!(
        fs.get("app/A.php").unwrap(),
        "<?php\n\nclass A\n{\n    /**\n     * @return list<int>\n     */\n    public function go(int $x): array\n    {\n        return [$x];\n    }\n\n    public function keep(): int\n    {\n        return 2;\n    }\n}\n"
    );
    let backup = format!(".callgraph/backups/app/A.php.{}.bak", stamp(T0));
    assert_eq!(
        out.lines,
        [format!("replaced app/A.php L5-11 → L5-11 (go) · backup: {backup}")]
    );
}

#[test]
fn replace_by_lines_and_the_expect_hash() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let lines_target = |expect: Option<&str>| Target::Replace {
        file: "app/A.php".into(),
        at: ReplaceAt::Lines(13, 16),
        expect: expect.map(str::to_owned),
    };
    let bad = run(&fs, &clock, &request("+keep():int{ return 9; }", false, lines_target(Some("deadbeef"))))
        .unwrap_err();
    assert!(bad.contains("changed"), "{bad}");
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);
    assert!(fs.names_under(".callgraph").is_empty());

    let current = crate::update::checksum::sha256_hex(
        "    public function keep(): int\n    {\n        return 2;\n    }".as_bytes(),
    );
    run(
        &fs,
        &clock,
        &request("+keep():int{ return 9; }", false, lines_target(Some(&current[..8]))),
    )
    .unwrap();
    assert!(fs.get("app/A.php").unwrap().contains("return 9;"));
}

#[test]
fn replace_reports_unknown_methods_and_bad_ranges() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let replace = |at: ReplaceAt| {
        run(
            &fs,
            &clock,
            &request(
                "+x():void",
                false,
                Target::Replace {
                    file: "app/A.php".into(),
                    at,
                    expect: None,
                },
            ),
        )
        .unwrap_err()
    };
    assert!(replace(ReplaceAt::Method { class: None, name: "nope".into() }).contains("no method"));
    assert!(replace(ReplaceAt::Lines(5, 400)).contains("bad line range"));
    assert!(replace(ReplaceAt::Lines(0, 3)).contains("bad line range"));
}

#[test]
fn a_syntax_error_writes_nothing() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let broken = "+f():int{ return ( ; }";
    let new_file = run(
        &fs,
        &clock,
        &request(
            &format!("c:Broken > {broken}"),
            false,
            Target::Write {
                file: "app/Broken.php".into(),
            },
        ),
    )
    .unwrap_err();
    assert!(new_file.contains("syntax error at L"), "{new_file}");
    assert!(fs.get("app/Broken.php").is_none());

    let before = fs.get("app/Services/Pay.php").unwrap();
    let edit = run(
        &fs,
        &clock,
        &request(
            broken,
            false,
            Target::Into {
                file: "app/Services/Pay.php".into(),
                class: None,
                after: None,
            },
        ),
    )
    .unwrap_err();
    assert!(edit.contains("syntax error"), "{edit}");
    assert_eq!(fs.get("app/Services/Pay.php").unwrap(), before);
    assert!(fs.names_under(".callgraph").is_empty());
}

#[test]
fn crlf_files_keep_their_line_endings() {
    let fs = MemFs::with(&[("app/C.php", "<?php\r\n\r\nclass C\r\n{\r\n}\r\n")]);
    let clock = FixedClock(Cell::new(T0));
    run(
        &fs,
        &clock,
        &request(
            "+a():void",
            false,
            Target::Into {
                file: "app/C.php".into(),
                class: None,
                after: None,
            },
        ),
    )
    .unwrap();
    let text = fs.get("app/C.php").unwrap();
    assert!(text.contains("{\r\n    public function a(): void\r\n"), "{text:?}");
    assert!(!text.replace("\r\n", "").contains('\n'), "{text:?}");
}

// ---- mentések, undo ----

#[test]
fn backups_keep_the_newest_ten_per_file() {
    let fs = MemFs::with(&[("app/A.php", "x")]);
    let clock = FixedClock(Cell::new(T0));
    let e = env(&fs, &clock);
    let backups = Backups::new(e);
    for i in 0..12u64 {
        clock.0.set(T0 + i);
        backups.save("app/A.php", &format!("v{i}")).unwrap();
    }
    let kept = backups.list("app/A.php");
    assert_eq!(kept.len(), 10);
    assert_eq!(kept[0].0, stamp(T0 + 11));
    assert_eq!(kept[9].0, stamp(T0 + 2));
    assert_eq!(fs.read(&kept[0].1).unwrap(), "v11");
    // egy másik fájl mentései nem keverednek bele
    backups.save("app/AB.php", "other").unwrap();
    assert_eq!(backups.list("app/A.php").len(), 10);
    assert_eq!(backups.list("app/AB.php").len(), 1);
}

#[test]
fn two_backups_in_one_second_do_not_overwrite_each_other() {
    let fs = MemFs::default();
    let clock = FixedClock(Cell::new(T0));
    let backups = Backups::new(env(&fs, &clock));
    let a = backups.save("a.php", "one").unwrap();
    let b = backups.save("a.php", "two").unwrap();
    assert_ne!(a, b);
    assert_eq!(fs.get(&a).unwrap(), "one");
    assert_eq!(fs.get(&b).unwrap(), "two");
}

#[test]
fn undo_restores_and_is_itself_undoable() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let edit = request(
        "+go():int{ return 7; }",
        false,
        Target::Replace {
            file: "app/A.php".into(),
            at: ReplaceAt::Method {
                class: None,
                name: "go".into(),
            },
            expect: None,
        },
    );
    run(&fs, &clock, &edit).unwrap();
    let edited = fs.get("app/A.php").unwrap();
    assert_ne!(edited, DOCUMENTED);
    let generator = Generator::new(env(&fs, &clock), PhpEmitter);

    let first = generator.undo(Some("app/A.php")).unwrap();
    assert!(first.lines[0].starts_with("restored app/A.php from .callgraph/backups/app/A.php."), "{:?}", first.lines);
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);

    // az undo is visszavonható: a következő undo a szerkesztett állapotot hozza vissza
    generator.undo(None).unwrap();
    assert_eq!(fs.get("app/A.php").unwrap(), edited);
}

#[test]
fn undo_without_a_backup_is_a_one_line_error() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let generator = Generator::new(env(&fs, &clock), PhpEmitter);
    let e = generator.undo(Some("app/Models/Order.php")).unwrap_err().to_string();
    assert!(e.contains("no backup of app/Models/Order.php"), "{e}");
    assert!(generator.undo(None).unwrap_err().to_string().contains("no backups yet"));
}

#[test]
fn protected_files_are_not_modified() {
    struct Guarded(MemFs);
    impl FileSystem for Guarded {
        fn read(&self, p: &Path) -> io::Result<String> {
            self.0.read(p)
        }
        fn write(&self, p: &Path, c: &str) -> io::Result<()> {
            self.0.write(p, c)
        }
        fn create_new(&self, p: &Path, c: &str) -> io::Result<()> {
            self.0.create_new(p, c)
        }
        fn exists(&self, p: &Path) -> bool {
            self.0.exists(p)
        }
        fn list(&self, d: &Path) -> io::Result<Vec<DirEntry>> {
            self.0.list(d)
        }
        fn remove(&self, p: &Path) -> io::Result<()> {
            self.0.remove(p)
        }
        fn is_protected(&self, _: &Path) -> bool {
            true
        }
    }
    let fs = Guarded(MemFs::with(&[("app/A.php", DOCUMENTED)]));
    let clock = FixedClock(Cell::new(T0));
    let env = Env {
        root: Path::new(ROOT),
        fs: &fs,
        clock: &clock,
    };
    let e = Generator::new(env, PhpEmitter)
        .run(&request(
            "+x():void",
            false,
            Target::Into {
                file: "app/A.php".into(),
                class: None,
                after: None,
            },
        ))
        .unwrap_err()
        .to_string();
    assert!(e.contains("protected"), "{e}");
    assert_eq!(fs.0.get("app/A.php").unwrap(), DOCUMENTED);
}

#[test]
fn the_stamp_is_utc_and_sortable() {
    assert_eq!(stamp(T0), "20231114T221320");
    assert!(stamp(T0 + 1) > stamp(T0));
}

#[test]
fn edit_helpers_extend_ranges_and_sort_imports() {
    use super::edit::{add_imports, extend_over_docs, split_lines};
    let (lines, eol) = split_lines("a\r\nb\r\n");
    assert_eq!((lines.len(), eol), (3, "\r\n"));
    let doc: Vec<String> = ["x", "#[Route('/')]", "/** d */", "public function f()"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(extend_over_docs(&doc, 4), 2);
    let mut file: Vec<String> = ["<?php", "", "namespace A;", "", "use B\\Zed;", "", "class C {}"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let added = add_imports(&mut file, 6, &[("B\\Alpha".into(), None), ("B\\Zed".into(), None)]);
    assert_eq!(added, 1);
    assert_eq!(file[4], "use B\\Alpha;");
    assert_eq!(file[5], "use B\\Zed;");
}

#[test]
fn the_emitter_trait_is_a_strategy() {
    // a tokenizáló és az AST nem tud a PHP-ról: ugyanaz a Program más emitterrel is kiírható
    struct Names;
    impl Emitter for Names {
        fn extension(&self) -> &'static str {
            "txt"
        }
        fn declaration<'a>(
            &'a self,
            decl: &'a super::ast::TypeDecl,
        ) -> impl Iterator<Item = String> + 'a {
            std::iter::once(format!("type {}", decl.name))
        }
        fn members<'a>(
            &'a self,
            _: TypeKind,
            members: &'a [super::ast::Member],
        ) -> impl Iterator<Item = String> + 'a {
            members.iter().map(|m| format!("member {}", m.name))
        }
        fn referenced_names(
            &self,
            _: &[super::ast::TypeDecl],
            _: &[super::ast::Member],
        ) -> std::collections::BTreeSet<String> {
            Default::default()
        }
        fn file(&self, _: &super::emitter::FileSpec<'_>, _: &[super::ast::TypeDecl]) -> String {
            String::new()
        }
        fn check(&self, _: &str, _: &str) -> Result<(), (u32, String)> {
            Ok(())
        }
        fn analyze(&self, _: &str, _: &str) -> crate::model::FileGraph {
            unreachable!("not needed")
        }
    }
    let program = parser::parse("c:A > +x():int").unwrap();
    let lines: Vec<String> = Names.declaration(&program.decls[0]).collect();
    assert_eq!(lines, ["type A"]);
    assert_eq!(Names.extension(), "txt");
}
