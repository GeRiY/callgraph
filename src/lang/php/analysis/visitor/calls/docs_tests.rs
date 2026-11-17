//! A docblock-típusforrások tesztjei: `@mixin`, `@var`, `@property`, `@param`, `@return`, `@method`.

use crate::lang::php::PhpBackend;
use crate::model::Call;

const HEAD: &str =
    "<?php\nnamespace App\\Http;\n\nuse App\\Models\\Book;\nuse App\\Models\\Order as Ord;\n\n";

fn calls(body: &str) -> Vec<Call> {
    let src = format!("{HEAD}{body}");
    PhpBackend::with_defaults()
        .analyze_source("a.php", src.as_bytes())
        .graph
        .calls
}

/// Az adott nevű hívások feloldott céljai, sorrendben (feloldatlan: `-`).
fn resolved(body: &str, name: &str) -> Vec<String> {
    calls(body)
        .into_iter()
        .filter(|c| c.target == name)
        .map(|c| c.resolved.unwrap_or_else(|| "-".into()))
        .collect()
}

const COVER: &str = "App\\Models\\Book::coverUrl";

fn cover(body: &str) -> Vec<String> {
    resolved(body, "coverUrl")
}

#[test]
fn mixin_forwards_unknown_this_calls_to_the_model() {
    let src = "/** @mixin \\App\\Models\\Book */\nclass BookResource extends JsonResource {\n  public function toArray($r) {\n    return ['cover' => $this->coverUrl(), 'own' => $this->own(), 'w' => $this->whenLoaded('x')];\n  }\n  public function own() { return 1; }\n}\n";
    assert_eq!(cover(src), vec![COVER.to_string()]);
    // a saját metódus és a JsonResource API nem a modellé
    assert_eq!(resolved(src, "own"), vec!["-".to_string()]);
    assert_eq!(resolved(src, "whenLoaded"), vec!["-".to_string()]);
    // importtal feloldott rövid név
    let short = "/** @mixin Book */\nclass R { function f() { return $this->coverUrl(); } }\n";
    assert_eq!(cover(short), vec![COVER.to_string()]);
}

#[test]
fn mixin_needs_exactly_one_class() {
    let two = "/**\n * @mixin Book\n * @mixin Ord\n */\nclass R { function f() { return $this->coverUrl(); } }\n";
    assert_eq!(cover(two), vec!["-".to_string()]);
    let none = "class R { function f() { return $this->coverUrl(); } }\n";
    assert_eq!(cover(none), vec!["-".to_string()]);
    // a mixin nem szivárog a következő osztályra
    let next =
        "/** @mixin Book */\nclass A {}\nclass B { function f() { return $this->coverUrl(); } }\n";
    assert_eq!(cover(next), vec!["-".to_string()]);
}

#[test]
fn var_on_assignment_and_inline_without_name() {
    let named = "class C { function f($query) {\n  /** @var Book $book */\n  $book = $query->first();\n  $book->coverUrl();\n} }\n";
    assert_eq!(cover(named), vec![COVER.to_string()]);
    let inline = "class C { function f($query) {\n  /** @var \\App\\Models\\Book */\n  $book = $query->first();\n  $book->coverUrl();\n} }\n";
    assert_eq!(cover(inline), vec![COVER.to_string()]);
    let nullable = "class C { function f($query) {\n  /** @var ?Book $b */\n  $b = $query->first();\n  $b->coverUrl();\n} }\n";
    assert_eq!(cover(nullable), vec![COVER.to_string()]);
    let union = "class C { function f($query) {\n  /** @var Book|null $b */\n  $b = $query->first();\n  $b->coverUrl();\n} }\n";
    assert_eq!(cover(union), vec![COVER.to_string()]);
}

#[test]
fn var_for_another_variable_or_ambiguous_type_is_ignored() {
    let other = "class C { function f($query) {\n  /** @var Book $other */\n  $book = $query->first();\n  $book->coverUrl();\n} }\n";
    assert_eq!(cover(other), vec!["-".to_string()]);
    let union = "class C { function f($query) {\n  /** @var Book|Ord $b */\n  $b = $query->first();\n  $b->coverUrl();\n} }\n";
    assert_eq!(cover(union), vec!["-".to_string()]);
    let scalar = "class C { function f($query) {\n  /** @var string $b */\n  $b = $query->first();\n  $b->coverUrl();\n} }\n";
    assert_eq!(cover(scalar), vec!["-".to_string()]);
}

#[test]
fn native_inference_wins_over_var() {
    let src = "class C { function f() {\n  /** @var Ord $b */\n  $b = new Book();\n  $b->coverUrl();\n} }\n";
    assert_eq!(cover(src), vec![COVER.to_string()]);
}

#[test]
fn var_on_foreach() {
    let value = "class C { function f($books) {\n  /** @var Book $b */\n  foreach ($books as $b) { $b->coverUrl(); }\n} }\n";
    assert_eq!(cover(value), vec![COVER.to_string()]);
    let key_value = "class C { function f($books) {\n  /** @var Book $b */\n  foreach ($books as $k => $b) { $b->coverUrl(); }\n} }\n";
    assert_eq!(cover(key_value), vec![COVER.to_string()]);
    for ty in [
        "Collection<Book>",
        "Collection<int, Book>",
        "Book[]",
        "list<Book>",
    ] {
        let src = format!(
            "class C {{ function f($books) {{\n  /** @var {ty} $books */\n  foreach ($books as $b) {{ $b->coverUrl(); }}\n}} }}\n"
        );
        assert_eq!(cover(&src), vec![COVER.to_string()], "{ty}");
    }
    let unknown = "class C { function f($books) {\n  /** @var array<string> $books */\n  foreach ($books as $b) { $b->coverUrl(); }\n} }\n";
    assert_eq!(cover(unknown), vec!["-".to_string()]);
}

#[test]
fn property_tags_type_this_and_other_receivers() {
    let src = "/**\n * @property Book $book\n * @property-read ?Ord $order\n */\nclass Shelf {\n  function f() { return $this->book->coverUrl(); }\n}\nclass User {\n  function g(Shelf $shelf) { return $shelf->book->coverUrl(); }\n}\n";
    assert_eq!(cover(src), vec![COVER.to_string(), COVER.to_string()]);
    // a natív property-típus erősebb
    let native = "/** @property Ord $book */\nclass Shelf {\n  private Book $book;\n  function f() { return $this->book->coverUrl(); }\n}\n";
    assert_eq!(cover(native), vec![COVER.to_string()]);
    // másik osztály nem örökli
    let leak = "/** @property Book $book */\nclass A {}\nclass B { function f() { return $this->book->coverUrl(); } }\n";
    assert_eq!(cover(leak), vec!["-".to_string()]);
}

#[test]
fn param_tag_types_untyped_parameters_only() {
    let src = "class C {\n  /**\n   * @param Book $book the book\n   * @param Ord|null $order\n   */\n  function f($book, $order) { $book->coverUrl(); $order->coverUrl(); }\n  /** @param Ord $book */\n  function g(Book $book) { $book->coverUrl(); }\n}\n";
    assert_eq!(
        cover(src),
        vec![
            COVER.to_string(),
            "App\\Models\\Order::coverUrl".to_string(),
            COVER.to_string()
        ]
    );
}

#[test]
fn return_tag_types_own_method_results() {
    let src = "class C {\n  /** @return Book|null */\n  function load($id) { return null; }\n  function native(): Book { return new Book(); }\n  function f() {\n    $this->load(1)->coverUrl();\n    $b = $this->load(2);\n    $b->coverUrl();\n    $this->native()->coverUrl();\n  }\n}\n";
    assert_eq!(
        cover(src),
        vec![COVER.to_string(), COVER.to_string(), COVER.to_string()]
    );
    // docblock nélküli, skalár visszatérésű metódus: nincs típus
    let none = "class C {\n  /** @return string */\n  function load() { return ''; }\n  function f() { $this->load()->coverUrl(); }\n}\n";
    assert_eq!(cover(none), vec!["-".to_string()]);
}

#[test]
fn method_tags_declare_methods_and_return_types() {
    let src = "/**\n * @method static Builder stocked()\n * @method Book find(int $id)\n */\nclass Shelf {\n  function f() {\n    $this->stocked();\n    $this->find(1)->coverUrl();\n  }\n}\n";
    assert_eq!(
        resolved(src, "stocked"),
        vec!["App\\Http\\Shelf::stocked".to_string()]
    );
    assert_eq!(
        resolved(src, "find"),
        vec!["App\\Http\\Shelf::find".to_string()]
    );
    assert_eq!(cover(src), vec![COVER.to_string()]);
    // a deklarált metódus nem megy a mixinhez
    let mixed = "/**\n * @mixin Book\n * @method Book find(int $id)\n */\nclass R { function f() { $this->find(1); $this->coverUrl(); } }\n";
    assert_eq!(
        resolved(mixed, "find"),
        vec!["App\\Http\\R::find".to_string()]
    );
    assert_eq!(cover(mixed), vec![COVER.to_string()]);
}
