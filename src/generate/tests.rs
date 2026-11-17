//! A generátor egységtesztjei: tokenizáló / parser, PHP-emitter, a feloldó-lánc, a szerkesztő-parancsok (memóriabeli
//! fájlrendszerrel és rögzített órával), a mentések és az undo.

use std::cell::Cell;
use std::io;
use std::path::Path;

use super::ast::TypeKind;
use super::backup::Backups;
use super::fs::{DirEntry, Env, FileSystem, stamp, walk_php};
use super::generator::{GenRequest, Generator, Mode, Output, Place, ReplaceAt, Target};
use super::php::PhpEmitter;
use super::project::Psr4;
use super::testing::{FixedClock, MemFs, ROOT};
use super::{Emitter, parser};

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
        mode: Mode::of(dry, false),
        target,
    }
}

/// A `--after` neve (vagy semmi) `Place`-ként.
fn place_after(name: Option<&str>) -> Place {
    name.map_or(Place::End, |m| Place::After(m.to_owned()))
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
    assert_eq!(
        TypeRef::parse("string").unwrap(),
        TypeRef::Named("string".into())
    );
    assert!(matches!(
        TypeRef::parse("int|string").unwrap(),
        TypeRef::Union(_)
    ));
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
    assert!(
        text.contains("public function b(): Generator\n    {\n        yield from [];"),
        "{text}"
    );
    assert!(
        text.contains("public function e(): void\n    {\n        // TODO"),
        "{text}"
    );
    assert!(
        text.contains("public function n()\n    {\n        // TODO"),
        "{text}"
    );
    assert!(
        text.contains("public function __construct(int $a)\n    {\n        // TODO"),
        "{text}"
    );
}

#[test]
fn neutral_types_map_to_php_with_docblocks() {
    let text = emit(
        "c:Svc > +c(): iter<int> + +d(list<Order> $orders, map<str,int> $m, opt<int> $o):list<Order> + +s(str $a, bytes $b, float $f, bool $c):mixed",
    );
    assert!(text.contains("     * @return iterable<int>\n     */\n    public function c(): iterable\n    {\n        yield from [];"), "{text}");
    assert!(text.contains("@param list<Order> $orders\n     * @param array<string, int> $m\n     * @return list<Order>\n     */\n    public function d(array $orders, array $m, ?int $o): array"), "{text}");
    assert!(
        text.contains("public function s(string $a, string $b, float $f, bool $c): mixed"),
        "{text}"
    );
}

#[test]
fn list_properties_get_a_var_docblock() {
    let text = emit("c:A > -$items:list<Order>=[]");
    assert!(
        text.contains("    /** @var list<Order> */\n    private array $items = [];"),
        "{text}"
    );
}

#[test]
fn long_parameter_lists_wrap_with_trailing_commas() {
    let text = emit(
        "c:A > +handle(string $firstArgument, string $secondArgument, string $thirdArgument, string $fourthArgument):void",
    );
    assert!(
        text.contains("    public function handle(\n        string $firstArgument,\n"),
        "{text}"
    );
    assert!(
        text.contains("        string $fourthArgument,\n    ): void {\n        // TODO\n    }"),
        "{text}"
    );
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

const COMPOSER: &str =
    r#"{"autoload":{"psr-4":{"App\\":"app/"}},"autoload-dev":{"psr-4":{"Tests\\":["tests/"]}}}"#;

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
    assert_eq!(
        map.namespace_of("app/Services/Pay.php").as_deref(),
        Some("App\\Services")
    );
    assert_eq!(map.namespace_of("app/Pay.php").as_deref(), Some("App"));
    assert_eq!(
        map.fqcn_of("app/Models/Order.php").as_deref(),
        Some("App\\Models\\Order")
    );
    assert_eq!(
        map.namespace_of("tests/Unit/PayTest.php").as_deref(),
        Some("Tests\\Unit")
    );
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
    // az elválasztó platformfüggő (Windows: `\`): normalizált összevetés
    let found: Vec<String> = walk_php(&fs, Path::new(ROOT).join("app"))
        .map(|p| p.display().to_string().replace('\\', "/"))
        .collect();
    assert_eq!(found, ["/p/app/A.php", "/p/app/Z/C.php"]);
    let first = walk_php(&fs, Path::new(ROOT).join("app"))
        .next()
        .map(|p| p.display().to_string().replace('\\', "/"));
    assert_eq!(first.as_deref(), Some("/p/app/A.php"));
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
    assert!(
        text.contains("use App\\Models\\Order;\nuse Vendor\\Lib\\Order as Ord;"),
        "{text}"
    );
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
    assert!(
        err.contains("already exists") && err.contains("--into"),
        "{err}"
    );
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
                place: Place::After("charge".into()),
                upsert: false,
                expect: None,
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
                place: Place::End,
                upsert: false,
                expect: None,
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
                place: Place::End,
                upsert: false,
                expect: None,
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
                    place: place_after(after),
                    upsert: false,
                    expect: None,
                },
            ),
        )
        .unwrap_err()
    };
    assert!(into(Some("Nope"), None).contains("no class `Nope`"));
    let e = into(None, Some("nope"));
    assert!(
        e.contains("no method `nope`") && e.contains("charge, other"),
        "{e}"
    );
}

/// `--into` az `app/A.php` osztályába, a megadott hellyel és móddal.
fn into_with(expr: &str, mode: Mode, place: Place, upsert: bool) -> GenRequest {
    GenRequest {
        expr: expr.to_owned(),
        mode,
        target: Target::Into {
            file: "app/A.php".into(),
            class: None,
            place,
            upsert,
            expect: None,
        },
    }
}

#[test]
fn into_before_a_method_goes_above_its_docblock() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let place = Place::Before("go".into());
    let dry = run(
        &fs,
        &clock,
        &into_with("+mid():int{ return 5; }", Mode::Dry, place.clone(), false),
    )
    .unwrap();
    assert_eq!(dry.lines[0], "→ app/A.php::A before go (L5)");
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);
    let out = run(
        &fs,
        &clock,
        &into_with("+mid():int{ return 5; }", Mode::Write, place, false),
    )
    .unwrap();
    assert!(out.lines[0].contains("A before go (L5-8)"), "{out:?}");
    let text = fs.get("app/A.php").unwrap();
    assert!(
        text.contains("{\n    public function mid(): int\n    {\n        return 5;\n    }\n\n    /**\n     * Old doc."),
        "{text}"
    );
}

#[test]
fn into_at_a_line_stays_inside_the_class_body() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let at = Place::Line;
    let dry = run(
        &fs,
        &clock,
        &into_with("+mid():int{ return 5; }", Mode::Dry, at(13), false),
    )
    .unwrap();
    assert_eq!(dry.lines[0], "→ app/A.php::A at L13 (L13)");
    for bad in [1, 3, 99] {
        let err = run(
            &fs,
            &clock,
            &into_with("+mid():int", Mode::Dry, at(bad), false),
        )
        .unwrap_err();
        assert!(err.contains("not inside the body"), "{err}");
    }
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);
}

#[test]
fn upsert_with_expect_checks_only_an_existing_member() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let with_expect = |expr: &str, expect: &str| GenRequest {
        expr: expr.to_owned(),
        mode: Mode::Write,
        target: Target::Into {
            file: "app/A.php".into(),
            class: None,
            place: Place::End,
            upsert: true,
            expect: Some(expect.to_owned()),
        },
    };
    // létező tag + hibás hash: a csere megtagadva, semmi nem íródik
    let err = run(
        &fs,
        &clock,
        &with_expect("+keep():int{ return 9; }", "deadbeef"),
    )
    .unwrap_err();
    assert!(err.contains("changed"), "{err}");
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);
    // új tag: az --expect figyelmen kívül marad, beszúrás történik
    let out = run(
        &fs,
        &clock,
        &with_expect("+fresh():int{ return 3; }", "deadbeef"),
    )
    .unwrap();
    assert!(out.lines[0].starts_with("inserted"), "{out:?}");
}

#[test]
fn into_at_the_opening_brace_line_is_rejected_upfront() {
    // Pay: `class Pay` a 9., a `{` a 10. sorban (Allman), a záró `}` a 19. sorban
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    for bad in [9, 10, 20, 99] {
        let err = run(
            &fs,
            &clock,
            &GenRequest {
                expr: "+late():void".to_owned(),
                mode: Mode::Write,
                target: Target::Into {
                    file: "app/Services/Pay.php".into(),
                    class: None,
                    place: Place::Line(bad),
                    upsert: false,
                    expect: None,
                },
            },
        )
        .unwrap_err();
        assert!(
            err.contains(&format!("--at L{bad} is not inside the body")),
            "{err}"
        );
        assert!(err.contains("(L11-L19)"), "{err}");
    }
    assert!(!fs.get("app/Services/Pay.php").unwrap().contains("late"));
}

#[test]
fn upsert_replaces_an_existing_member_and_inserts_a_new_one() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let out = run(
        &fs,
        &clock,
        &into_with("+keep():int{ return 9; }", Mode::Write, Place::End, true),
    )
    .unwrap();
    assert!(
        out.lines[0].starts_with("replaced app/A.php L13-16"),
        "{out:?}"
    );
    assert!(fs.get("app/A.php").unwrap().contains("return 9;"));
    let out = run(
        &fs,
        &clock,
        &into_with("+fresh():int{ return 7; }", Mode::Write, Place::End, true),
    )
    .unwrap();
    assert!(out.lines[0].starts_with("inserted 1 member(s)"), "{out:?}");
    assert!(
        fs.get("app/A.php")
            .unwrap()
            .contains("public function fresh(): int")
    );
}

#[test]
fn diff_shows_only_the_changed_lines_of_a_replace() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let req = GenRequest {
        expr: "+keep():int{ return 9; }".to_owned(),
        mode: Mode::Diff,
        target: Target::Replace {
            file: "app/A.php".into(),
            at: ReplaceAt::Method {
                class: None,
                name: "keep".into(),
            },
            expect: None,
        },
    };
    let out = run(&fs, &clock, &req).unwrap();
    assert_eq!(out.lines[0], "→ app/A.php replace L13-16 → L13-16 (keep)");
    assert!(
        out.lines.contains(&"-         return 2;".to_owned()),
        "{out:?}"
    );
    assert!(
        out.lines.contains(&"+         return 9;".to_owned()),
        "{out:?}"
    );
    assert!(
        !out.lines.iter().any(|l| l.contains("function keep")),
        "{out:?}"
    );
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);
}

#[test]
fn diff_helper_trims_the_common_edges() {
    let l = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        super::report::diff(&l(&["a", "b", "c"]), &l(&["a", "x", "c"])),
        l(&["  a", "- b", "+ x", "  c"])
    );
    assert_eq!(
        super::report::diff(&l(&["a"]), &l(&["a"])),
        l(&["= no change"])
    );
    let long: Vec<String> = (0..100).map(|i| format!("n{i}")).collect();
    let out = super::report::diff(&[], &long);
    assert_eq!(out.len(), 61);
    assert_eq!(out.last().map(String::as_str), Some("… +40 lines"));
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
        [format!(
            "replaced app/A.php L5-11 → L5-11 (go) · backup: {backup}"
        )]
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
    let bad = run(
        &fs,
        &clock,
        &request(
            "+keep():int{ return 9; }",
            false,
            lines_target(Some("deadbeef")),
        ),
    )
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
        &request(
            "+keep():int{ return 9; }",
            false,
            lines_target(Some(&current[..8])),
        ),
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
    assert!(
        replace(ReplaceAt::Method {
            class: None,
            name: "nope".into()
        })
        .contains("no method")
    );
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
                place: Place::End,
                upsert: false,
                expect: None,
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
                place: Place::End,
                upsert: false,
                expect: None,
            },
        ),
    )
    .unwrap();
    let text = fs.get("app/C.php").unwrap();
    assert!(
        text.contains("{\r\n    public function a(): void\r\n"),
        "{text:?}"
    );
    assert!(!text.replace("\r\n", "").contains('\n'), "{text:?}");
}

// ---- mentések, undo ----

#[test]
fn backups_keep_the_newest_twenty_per_file() {
    let fs = MemFs::with(&[("app/A.php", "x")]);
    let clock = FixedClock(Cell::new(T0));
    let e = env(&fs, &clock);
    let backups = Backups::new(e);
    for i in 0..22u64 {
        clock.0.set(T0 + i);
        backups.save("app/A.php", &format!("v{i}")).unwrap();
    }
    let kept = backups.list("app/A.php");
    assert_eq!(kept.len(), 20);
    assert_eq!(kept[0].0, stamp(T0 + 21));
    assert_eq!(kept[19].0, stamp(T0 + 2));
    assert_eq!(fs.read(&kept[0].1).unwrap(), "v21");
    // egy másik fájl mentései nem keverednek bele
    backups.save("app/AB.php", "other").unwrap();
    assert_eq!(backups.list("app/A.php").len(), 20);
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
fn undo_restores_the_previous_state() {
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
    assert!(
        first.lines[0].starts_with("restored app/A.php from .callgraph/backups/app/A.php."),
        "{:?}",
        first.lines
    );
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);

    // verem: nincs több lépés, az undo nem kapcsol vissza a szerkesztett állapotra
    assert!(generator.undo(None).is_err());
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);
    assert_ne!(edited, DOCUMENTED);
}

#[test]
fn undo_is_a_multi_step_stack() {
    let fs = MemFs::with(&[("app/A.php", DOCUMENTED)]);
    let clock = FixedClock(Cell::new(T0));
    let edit = |body: &str| {
        request(
            &format!("+go():int{{ return {body}; }}"),
            false,
            Target::Replace {
                file: "app/A.php".into(),
                at: ReplaceAt::Method {
                    class: None,
                    name: "go".into(),
                },
                expect: None,
            },
        )
    };
    run(&fs, &clock, &edit("1")).unwrap();
    let after_a1 = fs.get("app/A.php").unwrap();
    run(&fs, &clock, &edit("2")).unwrap();
    let after_a2 = fs.get("app/A.php").unwrap();
    assert_ne!(after_a1, after_a2);
    let generator = Generator::new(env(&fs, &clock), PhpEmitter);
    generator.undo(None).unwrap();
    assert_eq!(fs.get("app/A.php").unwrap(), after_a1);
    generator.undo(None).unwrap();
    assert_eq!(fs.get("app/A.php").unwrap(), DOCUMENTED);
    let e = generator.undo(None).unwrap_err().to_string();
    assert!(e.contains("no backups yet"), "{e}");
}

#[test]
fn undo_without_a_backup_is_a_one_line_error() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let generator = Generator::new(env(&fs, &clock), PhpEmitter);
    let e = generator
        .undo(Some("app/Models/Order.php"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no backup of app/Models/Order.php"), "{e}");
    assert!(
        generator
            .undo(None)
            .unwrap_err()
            .to_string()
            .contains("no backups yet")
    );
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
                place: Place::End,
                upsert: false,
                expect: None,
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
    let mut file: Vec<String> = [
        "<?php",
        "",
        "namespace A;",
        "",
        "use B\\Zed;",
        "",
        "class C {}",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let added = add_imports(
        &mut file,
        6,
        &[("B\\Alpha".into(), None), ("B\\Zed".into(), None)],
    );
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

/// Nyelvfüggetlen paraméter: a név `$` nélkül is jó; a típus, az alapérték, a `...` és a `&` megmarad.
#[test]
fn params_accept_names_without_a_dollar() {
    let params = super::head::parse_params(
        "-Request request, int retries=3, list<int> items, ...rest, &out",
    )
    .unwrap();
    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["request", "retries", "items", "rest", "out"]);
    assert!(params[0].promote.is_some());
    assert_eq!(params[1].default.as_deref(), Some("3"));
    assert!(params[3].variadic && params[4].by_ref);
    // a `$`-os PHP-alak változatlanul működik
    let php = super::head::parse_params("Order $order = null").unwrap();
    assert_eq!(
        (php[0].name.as_str(), php[0].default.as_deref()),
        ("order", Some("null"))
    );
}

/// A közös fixture számolja a `read` / `list` hívásokat (az IO-regressziós assertek alapja).
#[test]
fn the_fixture_counts_reads_and_lists() {
    let fs = MemFs::with(&[("app/A.php", "<?php\n")]);
    assert_eq!((fs.read_calls(), fs.list_calls()), (0, 0));
    fs.read(&Path::new(ROOT).join("app/A.php")).unwrap();
    fs.list(&Path::new(ROOT).join("app")).unwrap();
    fs.list(&Path::new(ROOT).join("app")).unwrap();
    assert_eq!((fs.read_calls(), fs.list_calls()), (1, 2));
    fs.reset_counters();
    assert_eq!((fs.read_calls(), fs.list_calls()), (0, 0));
}

/// A közös ellenőrző a kiterjesztés szerint választ; ismeretlen kiterjesztésnél nem ellenőriz.
#[test]
fn the_checker_follows_the_extension() {
    use super::target::Checker;
    assert!(
        Checker::for_path("app/A.php")
            .check("<?php\nclass A {}\n")
            .is_ok()
    );
    assert!(
        Checker::for_path("app/A.php")
            .check("<?php\nclass {\n")
            .is_err()
    );
    assert!(
        Checker::for_path("a.ts")
            .check("const a: number = 1;\n")
            .is_ok()
    );
    assert!(Checker::for_path("a.ts").check("const = ;\n").is_err());
    assert!(
        Checker::for_path("a.html")
            .check("<body>\n<div>\n</body>\n")
            .is_err()
    );
    assert!(
        Checker::for_path("a.vue")
            .check("<template>\n<div>\n</template>\n")
            .is_err()
    );
    assert_eq!(Checker::for_path("notes.txt"), Checker::Skip);
    assert!(Checker::for_path("notes.txt").check("<<<").is_ok());
}

/// A `--dry` `--into` / `--replace` célnál egy hely-fejléc sort ír a kód elé; a hely egyezik a valódi futás összegzésével.
#[test]
fn dry_into_and_replace_print_a_placement_header() {
    let into = Target::Into {
        file: "app/Services/Pay.php".into(),
        class: None,
        place: Place::After("charge".into()),
        upsert: false,
        expect: None,
    };
    let clock = FixedClock(Cell::new(T0));
    let dry = run(
        &project(),
        &clock,
        &request("+refund():void{ }", true, into.clone()),
    )
    .unwrap();
    assert!(
        dry.lines[0].starts_with("→ app/Services/Pay.php::Pay after charge (L"),
        "{:?}",
        dry.lines
    );
    assert!(dry.lines[1].contains("function refund"), "{:?}", dry.lines);
    let real = run(
        &project(),
        &clock,
        &request("+refund():void{ }", false, into),
    )
    .unwrap();
    // `inserted 1 member(s) into … (L15-19)` ↔ `→ … (L15)`
    let start = real.lines[0]
        .rsplit_once("(L")
        .and_then(|(_, rest)| rest.split_once('-'))
        .map(|(a, _)| a.to_owned())
        .unwrap();
    assert!(
        dry.lines[0].ends_with(&format!("(L{start})")),
        "{} / {}",
        dry.lines[0],
        real.lines[0]
    );

    let replace = Target::Replace {
        file: "app/Services/Pay.php".into(),
        at: ReplaceAt::Method {
            class: None,
            name: "other".into(),
        },
        expect: None,
    };
    let dry = run(
        &project(),
        &clock,
        &request("+other():int{ return 2; }", true, replace),
    )
    .unwrap();
    assert!(
        dry.lines[0].starts_with("→ app/Services/Pay.php replace L"),
        "{:?}",
        dry.lines
    );
    assert!(dry.lines[0].ends_with("(other)"), "{:?}", dry.lines);
    assert!(dry.lines[1].contains("function other"), "{:?}", dry.lines);
    // --write / előnézet: továbbra is csak a kód
    let out = run(&project(), &clock, &request("c:A", true, Target::Preview)).unwrap();
    assert!(!out.lines[0].starts_with('→'), "{:?}", out.lines);
}

/// A hibák fajtája: létező fájl, hiányzó fájl, szintaktikai hiba, megváltozott sorok.
#[test]
fn errors_carry_a_kind() {
    use super::error::Kind;
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    let generator = Generator::new(env(&fs, &clock), PhpEmitter);
    let kind_of = |expr: &str, target: Target| {
        generator
            .run(&request(expr, false, target))
            .unwrap_err()
            .kind()
    };
    assert_eq!(
        kind_of(
            "c:Order",
            Target::Write {
                file: "app/Models/Order.php".into()
            }
        ),
        Kind::Exists
    );
    assert_eq!(
        kind_of(
            "+x():void",
            Target::Into {
                file: "app/Missing.php".into(),
                class: None,
                place: Place::End,
                upsert: false,
                expect: None
            }
        ),
        Kind::NotFound
    );
    assert_eq!(
        kind_of(
            "+x():void{ if }",
            Target::Into {
                file: "app/Services/Pay.php".into(),
                class: None,
                place: Place::End,
                upsert: false,
                expect: None
            }
        ),
        Kind::Syntax
    );
    assert_eq!(
        kind_of(
            "+x():void",
            Target::Replace {
                file: "app/Services/Pay.php".into(),
                at: ReplaceAt::Lines(9, 10),
                expect: Some("deadbeef".into())
            }
        ),
        Kind::Changed
    );
}

/// Több ismeretlen név egy `--into`-ban: a projekt-bejárás egyszer történik (nem nevenként), és minden fájlt
/// legfeljebb egyszer olvasunk.
#[test]
fn several_unknown_names_share_one_project_walk() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    fs.reset_counters();
    let out = run(
        &fs,
        &clock,
        &request(
            "+f(Alpha $a, Beta $b, Gamma $c, Delta $d):void",
            true,
            Target::Into {
                file: "app/Services/Pay.php".into(),
                class: None,
                place: Place::End,
                upsert: false,
                expect: None,
            },
        ),
    )
    .unwrap();
    assert_eq!(out.unresolved, ["Alpha", "Beta", "Delta", "Gamma"]);
    // a project(): app, app/Dto, app/Models, app/Services + egy gyökér-listázás — egy bejárás (5 listázás) a nevek
    // számától függetlenül; a régi névenkénti bejárás 4 névnél 4 × 4 lenne
    assert!(fs.list_calls() <= 5, "lists: {}", fs.list_calls());
    for rel in ["composer.json", "app/Other.php", "app/Models/Order.php"] {
        assert!(fs.reads_of(rel) <= 1, "{rel}: {}", fs.reads_of(rel));
    }
    assert!(fs.read_calls() <= 6, "reads: {}", fs.read_calls());
}

/// A `--write` is egy bejáráson osztozik a névfeloldás és a `strict_types` mintavétel között.
#[test]
fn write_shares_the_walk_between_names_and_strict_detection() {
    let fs = project();
    let clock = FixedClock(Cell::new(T0));
    fs.reset_counters();
    let out = run(
        &fs,
        &clock,
        &request(
            "c:Fresh > +f(Alpha $a, Beta $b):void",
            true,
            Target::Write {
                file: "app/Services/Fresh.php".into(),
            },
        ),
    )
    .unwrap();
    assert_eq!(out.unresolved, ["Alpha", "Beta"]);
    assert!(fs.list_calls() <= 5, "lists: {}", fs.list_calls());
    assert!(fs.reads_of("app/Other.php") <= 1);
    assert!(fs.reads_of("composer.json") <= 1);
}

// ---- trait-használat, névtelen osztály, nyers blokk, Laravel-nevek ----

const LARAVEL_COMPOSER: &str =
    r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#;

fn laravel_project() -> MemFs {
    MemFs::with(&[
        ("composer.json", LARAVEL_COMPOSER),
        (
            "app/Http/Controllers/PostController.php",
            "<?php\n\nnamespace App\\Http\\Controllers;\n\nclass PostController {}\n",
        ),
        (
            "routes/web.php",
            "<?php\n\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/', fn () => 1);\n",
        ),
    ])
}

#[test]
fn a_type_takes_trait_uses() {
    let text = emit("c:Post<Model> > use:HasFactory,Notifiable + +a():void");
    assert!(
        text.contains("{\n    use HasFactory, Notifiable;\n\n    public function a(): void"),
        "{text}"
    );
}

#[test]
fn an_anonymous_class_returns_new_class() {
    let text = emit("a:<Migration> > +up():void{ run(); }");
    assert!(
        text.starts_with("return new class extends Migration\n{\n"),
        "{text}"
    );
    assert!(text.ends_with("\n};"), "{text}");
}

#[test]
fn laravel_framework_names_resolve_only_in_laravel_projects() {
    let clock = FixedClock(Cell::new(T0));
    let fs = laravel_project();
    let out = run(
        &fs,
        &clock,
        &request(
            "a:<Migration> > +up():void{ Schema::create('t', function (Blueprint $t) { $t->id(); }); }",
            true,
            Target::Write {
                file: "database/migrations/2026_01_01_000000_create_t_table.php".into(),
            },
        ),
    )
    .unwrap();
    let text = out.lines.join("\n");
    assert!(
        text.contains("use Illuminate\\Database\\Migrations\\Migration;"),
        "{text}"
    );
    assert!(
        text.contains("use Illuminate\\Database\\Schema\\Blueprint;"),
        "{text}"
    );
    assert!(
        text.contains("use Illuminate\\Support\\Facades\\Schema;"),
        "{text}"
    );
    assert!(out.unresolved.is_empty(), "{:?}", out.unresolved);
    let plain = project();
    let out = run(
        &plain,
        &clock,
        &request("c:A<Model>", true, Target::Preview),
    )
    .unwrap();
    assert_eq!(out.unresolved, ["Model"]);
}

#[test]
fn a_raw_php_block_is_written_with_header_and_imports() {
    let clock = FixedClock(Cell::new(T0));
    let fs = laravel_project();
    run(
        &fs,
        &clock,
        &request(
            "php{ Route::get('/posts', [PostController::class, 'index']); }",
            false,
            Target::Write {
                file: "routes/extra.php".into(),
            },
        ),
    )
    .unwrap();
    let text = fs.get("routes/extra.php").unwrap();
    assert_eq!(
        text,
        "<?php\n\nuse App\\Http\\Controllers\\PostController;\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/posts', [PostController::class, 'index']);\n"
    );
}

fn into_web(place: Place) -> Target {
    Target::Into {
        file: "routes/web.php".into(),
        class: None,
        place,
        upsert: false,
        expect: None,
    }
}

#[test]
fn a_raw_php_block_goes_into_a_file_end_a_line_and_replaces_a_range() {
    let clock = FixedClock(Cell::new(T0));
    let fs = laravel_project();
    run(
        &fs,
        &clock,
        &request(
            "php{\n    Route::get('/posts', [PostController::class, 'index']);\n    Route::get('/about', fn () => 2);\n}",
            false,
            into_web(Place::End),
        ),
    )
    .unwrap();
    let text = fs.get("routes/web.php").unwrap();
    assert!(
        text.contains("use App\\Http\\Controllers\\PostController;"),
        "{text}"
    );
    assert!(
        text.ends_with("Route::get('/about', fn () => 2);\n"),
        "{text}"
    );
    assert!(
        text.contains("Route::get('/', fn () => 1);\n\nRoute::get('/posts'"),
        "{text}"
    );

    run(
        &fs,
        &clock,
        &request(
            "php{ Route::get('/home', fn () => 3); }",
            false,
            Target::Replace {
                file: "routes/web.php".into(),
                at: ReplaceAt::Lines(6, 6),
                expect: None,
            },
        ),
    )
    .unwrap();
    let text = fs.get("routes/web.php").unwrap();
    assert!(text.contains("Route::get('/home', fn () => 3);"), "{text}");
    assert!(!text.contains("Route::get('/', fn () => 1);"), "{text}");

    run(
        &fs,
        &clock,
        &request(
            "php{ Route::get('/x', fn () => 4); }",
            false,
            into_web(Place::After("L6".into())),
        ),
    )
    .unwrap();
    assert!(fs.get("routes/web.php").unwrap().contains("'/x'"));

    let err = run(
        &fs,
        &clock,
        &request(
            "php{ Route::get('/y', fn () => 5); }",
            false,
            into_web(Place::After("index".into())),
        ),
    )
    .unwrap_err();
    assert!(err.contains("L<n>"), "{err}");
}

#[test]
fn a_raw_block_in_a_blade_view_is_verbatim_and_unchecked() {
    let clock = FixedClock(Cell::new(T0));
    let fs = laravel_project();
    run(
        &fs,
        &clock,
        &request(
            "raw{\n<!DOCTYPE html>\n<html>\n<head>@vite(['resources/js/app.js'])</head>\n<body>@inertia</body>\n</html>\n}",
            false,
            Target::Write {
                file: "resources/views/app.blade.php".into(),
            },
        ),
    )
    .unwrap();
    assert_eq!(
        fs.get("resources/views/app.blade.php").unwrap(),
        "<!DOCTYPE html>\n<html>\n<head>@vite(['resources/js/app.js'])</head>\n<body>@inertia</body>\n</html>\n"
    );
}

#[test]
fn a_raw_block_cannot_be_mixed_with_types() {
    assert!(parser::parse("php{ run(); } + c:A").is_err());
}

#[test]
fn a_raw_php_block_takes_the_indent_of_the_line_it_replaces() {
    let clock = FixedClock(Cell::new(T0));
    let fs = laravel_project();
    fs.put(
        "bootstrap/x.php",
        "<?php\n\nreturn function () {\n    //\n};\n",
    );
    run(
        &fs,
        &clock,
        &request(
            "php{ $a = 1;\n$b = 2; }",
            false,
            Target::Replace {
                file: "bootstrap/x.php".into(),
                at: ReplaceAt::Lines(4, 4),
                expect: None,
            },
        ),
    )
    .unwrap();
    assert_eq!(
        fs.get("bootstrap/x.php").unwrap(),
        "<?php\n\nreturn function () {\n    $a = 1;\n    $b = 2;\n};\n"
    );
}

#[test]
fn an_empty_raw_block_deletes_a_range_with_replace_only() {
    let clock = FixedClock(Cell::new(T0));
    let fs = laravel_project();
    fs.put("routes/y.php", "<?php\n\nfoo();\nbar();\nbaz();\n");
    run(
        &fs,
        &clock,
        &request(
            "php{}",
            false,
            Target::Replace {
                file: "routes/y.php".into(),
                at: ReplaceAt::Lines(4, 4),
                expect: None,
            },
        ),
    )
    .unwrap();
    assert_eq!(fs.get("routes/y.php").unwrap(), "<?php\n\nfoo();\nbaz();\n");
    let err = run(&fs, &clock, &request("php{}", false, into_web(Place::End)));
    assert!(err.is_err());
}
