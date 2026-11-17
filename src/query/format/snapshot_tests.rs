//! Pillanatkép-tesztek a nem-kód fájlok outline-jára (CSS, SCSS, JSON, Markdown, HTML): kis fixture, a teljes
//! outline-törzs soronként rögzítve (nem `contains`), hogy a motor-refaktor kimenet-megőrzését igazolják. A SCSS
//! a szándékos javításokat (interpolált szelektor, újraértékadás, `@media` metódus) tartalmazza.

use super::FormatterRegistry;
use crate::core::SourceFile;
use crate::i18n::Lang;
use crate::lang::LanguageRegistry;
use crate::query::QueryResult;

/// Az outline törzse a `FILE` sor és a záró 💡 sor nélkül.
fn body(path: &str, text: &str) -> String {
    let language = LanguageRegistry::with_defaults();
    let file = SourceFile::new(path, text.as_bytes().to_vec(), None);
    let analyzer = language.for_path(std::path::Path::new(path)).unwrap();
    let rows = [QueryResult::new(
        analyzer.analyze(&file).unwrap().graph,
        None,
    )];
    let out = FormatterRegistry::with_defaults()
        .in_lang(Lang::En)
        .render_with("outline", &rows, None, None)
        .unwrap();
    let start = out.find("\n\n").map_or(0, |i| i + 2);
    let end = out.rfind("\n---\n").unwrap_or(out.len());
    out[start..end].to_owned()
}

/// Egy hívás-kategória sora a metódus alatt (`    {kategória:<18}{lista}`).
fn calls(category: &str, list: &str) -> String {
    format!("    {category:<18}{list}\n")
}

#[test]
fn css_outline_snapshot() {
    let source = "@import 'base.css';
:root {
  --brand: #333;
}
/* card */
.card {
  color: var(--brand);
  margin: calc(1px + 2px);
}
@media (min-width: 640px) {
  .card .title { font-size: 2rem; }
}
";
    let expected = [
        "stylesheet app  L1-12\n",
        "  constants (1):\n",
        "    --brand = #333\n",
        "  @import 'base.css'  L1-1\n",
        "  :root  L2-4\n",
        "  .card  L6-9\n",
        &calls("function", "calc"),
        &calls("var", "var(--brand)"),
        "  @media (min-width: 640px)  L10-12\n",
        "  .card .title  L11-11\n",
    ]
    .concat();
    assert_eq!(body("app.css", source), expected);
}

#[test]
fn scss_outline_snapshot() {
    let source = "@use \"sass:math\";
$gap: 4px;
$n: 1;
$n: $n + 1;

@mixin pad($n) {
  padding: math.div($gap, $n);
}

.card {
  @include pad(2);
  &--#{$name} {
    color: darken($brand, 10%);
  }
}

@media (min-width: 640px) {
  .card-title { margin: 0; }
}
";
    // szándékos javítások: `&--#{$name}` egészben; a `$n: $n + 1` írás, nem második konstans; az `@media` metódus
    let expected = [
        "stylesheet app  L1-19\n",
        "  constants (2):\n",
        "    $gap = 4px\n",
        "    $n = 1\n",
        "  pad()  L6-8\n",
        &calls("function", "div"),
        "  .card  L10-15\n",
        &calls("function", "pad"),
        &calls("include", "pad"),
        &calls("interpolation", "$name"),
        "  &--#{$name}  L12-14\n",
        &calls("function", "darken"),
        "  @media (min-width: 640px)  L17-19\n",
        "  .card-title  L18-18\n",
    ]
    .concat();
    assert_eq!(body("app.scss", source), expected);
}

#[test]
fn json_outline_snapshot() {
    let source = "{
  \"name\": \"demo\",
  \"version\": \"1.2.0\",
  \"scripts\": {
    \"build\": \"vite build\"
  },
  \"dependencies\": {
    \"vue\": \"^3.4.0\"
  }
}
";
    let expected = "document package.json  L1-10
  constants (6):
    name = \"demo\"
    version = \"1.2.0\"
    scripts = object (1)
    scripts.build = \"vite build\"
    dependencies = object (1)
    dependencies.vue = \"^3.4.0\"
";
    assert_eq!(body("package.json", source), expected);
}

#[test]
fn markdown_outline_snapshot_merges_repeated_links() {
    let source = "# Guide
Docs: [site](https://example.com), [mail](mailto:a@b.c), [again](https://example.com).
## Setup
Back to [top](#guide) and [setup](#setup).
";
    let expected = [
        "document guide.md  L1-4\n",
        "  # Guide  L1-4\n",
        &calls("external_url", "https://example.com ×2, mailto:a@b.c"),
        "    ## Setup  L3-4\n",
        &calls("anchor", "#guide, #setup"),
    ]
    .concat();
    assert_eq!(body("docs/guide.md", source), expected);
}

#[test]
fn markdown_link_check_marks_missing_files_and_anchors() {
    // a fájl a repó gyökerében: a `Cargo.toml` létezik, a `nope.md` nem
    let path = format!("{}/guide.md", env!("CARGO_MANIFEST_DIR"));
    let source = "# Guide
See [ok](Cargo.toml), [gone](nope.md), [self](#guide), [bad](#nowhere), [site](https://example.com).
";
    let expected = [
        "document guide.md  L1-2\n",
        "  # Guide  L1-2\n",
        &calls("anchor", "#guide, #nowhere ⚠ missing"),
        &calls("external_url", "https://example.com"),
        &calls("internal_file", "Cargo.toml, nope.md ⚠ missing"),
    ]
    .concat();
    assert_eq!(body(&path, source), expected);
}

#[test]
fn html_outline_snapshot() {
    let source = "<html><head>
<title>Demo</title>
</head><body>
<h1>Hello</h1>
<p>Welcome <a href=\"https://example.com\">site</a></p>
<h2>More</h2>
<img src=\"logo.png\">
</body></html>
";
    let expected = [
        "document page.html  L1-8\n",
        "  <title> Demo  L2-2\n",
        "  # Hello  L4-8\n",
        &calls("external_url", "https://example.com"),
        "    ## More  L6-8\n",
        &calls("image", "logo.png"),
    ]
    .concat();
    assert_eq!(body("docs/page.html", source), expected);
}
