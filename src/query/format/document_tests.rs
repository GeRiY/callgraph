//! A nem-kód fájlok outline-ja: a Markdown- / HTML-címsor a szintje szerint behúzva, `#`-jelekkel (a HTML `<title>`
//! `<title> …` alakban); a CSS / SCSS szabály és
//! at-szabály `()` nélkül (a mixin / function marad hívható); a JSON-kulcs a rövid értékével.

use super::FormatterRegistry;
use crate::core::SourceFile;
use crate::i18n::Lang;
use crate::lang::LanguageRegistry;
use crate::query::QueryResult;

fn outline(path: &str, text: &str) -> String {
    render("outline", path, text)
}

fn render(format: &str, path: &str, text: &str) -> String {
    let language = LanguageRegistry::with_defaults();
    let file = SourceFile::new(path, text.as_bytes().to_vec(), None);
    let analyzer = language.for_path(std::path::Path::new(path)).unwrap();
    let rows = [QueryResult::new(
        analyzer.analyze(&file).unwrap().graph,
        None,
    )];
    FormatterRegistry::with_defaults()
        .in_lang(Lang::En)
        .render_with(format, &rows, None, None)
        .unwrap()
}

/// `{"key_0": "v0", …}` sorokra bontva: soronként egy kulcs.
fn flat_json(keys: usize) -> String {
    let rows: Vec<String> = (0..keys)
        .map(|i| format!("  \"key_{i}\": \"value {i}\""))
        .collect();
    format!("{{\n{}\n}}\n", rows.join(",\n"))
}

#[test]
fn json_method_index_falls_back_to_a_compact_key_index() {
    let out = render("method-index", "lang/en.json", &flat_json(450));
    assert!(out.contains("document en.json  L1-452\n"), "{out}");
    assert!(
        out.contains("  key_0 … key_449  450 string  L2-451\n"),
        "{out}"
    );
    assert!(out.lines().count() <= 25, "{out}");
}

#[test]
fn json_key_index_keeps_nested_objects_and_counts_their_keys() {
    let source = r#"{
  "name": "x",
  "board": {
    "title": "T",
    "cols": { "a": 1 }
  },
  "version": 3
}
"#;
    let out = render("method-index", "package.json", source);
    assert!(out.contains("  name  \"x\"  L2\n"), "{out}");
    assert!(out.contains("  board  object (2)  L3-5  4 keys\n"), "{out}");
    assert!(out.contains("  version  3  L7\n"), "{out}");
    assert!(!out.contains("board.title"), "{out}");
}

#[test]
fn summary_counts_keys_sections_and_rules_instead_of_zero_methods() {
    let json = render("summary", "lang/en.json", &flat_json(450));
    assert!(
        json.contains("en.json  document en.json  (450 keys)"),
        "{json}"
    );
    let md = render("summary", "docs/README.md", "# A\ntext\n## B\nmore\n");
    assert!(md.contains("(2 sections)"), "{md}");
    let css = render(
        "summary",
        "app.css",
        ".a { color: red; }\n.b { color: blue; }\n",
    );
    assert!(css.contains("stylesheet app  (2 rules)"), "{css}");
}

#[test]
fn stylesheet_outline_shows_calls_under_their_rule() {
    let css = outline(
        "app.css",
        ":root { --brand: red; }\n.card {\n  color: var(--brand);\n}\n",
    );
    let card = css.find("  .card").unwrap_or(usize::MAX);
    let var = css.find("var").unwrap_or(0);
    assert!(card < var, "{css}");
    let scss = outline(
        "app.scss",
        "@mixin pad($n) { padding: $n; }\n.card {\n  @include pad(2);\n}\n",
    );
    let card = scss.find("  .card").unwrap_or(usize::MAX);
    let include = scss.rfind("include").unwrap_or(0);
    assert!(card < include, "{scss}");
}

#[test]
fn markdown_headings_are_indented_by_level_without_parens() {
    let source = "# Title
intro
## Docker / build
text
### Sub
more
## Other
end
";
    let expected = "document README.md  L1-8
  # Title  L1-8
    ## Docker / build  L3-6
      ### Sub  L5-6
    ## Other  L7-8
";
    let out = outline("docs/README.md", source);
    assert!(out.contains(expected), "{out}");
    assert!(!out.contains("()"), "{out}");
}

#[test]
fn css_rules_and_at_rules_have_no_parens() {
    let source = "@import 'tailwindcss';

@theme {
  --font-sans: Inter;
}

.p-toast {
  z-index: 1;
}
";
    let out = outline("resources/css/app.css", source);
    for line in [
        "  @import 'tailwindcss'  L1-1\n",
        "  @theme  L3-5\n",
        "  .p-toast  L7-9\n",
        "    --font-sans = Inter\n",
    ] {
        assert!(out.contains(line), "{line:?}\n{out}");
    }
    assert!(!out.contains("()"), "{out}");
}

#[test]
fn scss_rules_lose_parens_but_mixins_keep_them() {
    let source = "@mixin card($pad) {
  padding: $pad;
}
.card {
  @include card(1rem);
}
";
    let out = outline("assets/app.scss", source);
    assert!(out.contains("  card()  L1-3\n"), "{out}");
    assert!(out.contains("  .card  L4-6\n"), "{out}");
}

#[test]
fn json_keys_show_short_scalar_values() {
    let source = r#"{
  "private": true,
  "scripts": {
    "build": "vite build"
  },
  "files": ["a", "b"],
  "version": 3,
  "nothing": null
}
"#;
    let out = outline("package.json", source);
    for line in [
        "    private = true\n",
        "    scripts = object (1)\n",
        "    scripts.build = \"vite build\"\n",
        "    files = array (2)\n",
        "    version = 3\n",
        "    nothing = null\n",
    ] {
        assert!(out.contains(line), "{line:?}\n{out}");
    }
}

#[test]
fn html_title_and_headings_are_outlined_like_markdown() {
    let source = "<html><head>
<title>Design doc</title>
</head><body>
<h1>Overview</h1>
<p>text</p>
<h2>Goals</h2>
<h2>Risks</h2>
</body></html>
";
    let expected = "document design.html  L1-8
  <title> Design doc  L2-2
  # Overview  L4-8
    ## Goals  L6-6
    ## Risks  L7-8
";
    let out = outline("docs/design.html", source);
    assert!(out.contains(expected), "{out}");
    assert!(!out.contains("()"), "{out}");
}
