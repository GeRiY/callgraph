//! A nem-kód fájlok outline-ja: a Markdown- / HTML-címsor a szintje szerint behúzva, `#`-jelekkel (a HTML `<title>`
//! `<title> …` alakban); a CSS / SCSS szabály és
//! at-szabály `()` nélkül (a mixin / function marad hívható); a JSON-kulcs a rövid értékével.

use super::FormatterRegistry;
use crate::core::SourceFile;
use crate::i18n::Lang;
use crate::lang::LanguageRegistry;
use crate::query::QueryResult;

fn outline(path: &str, text: &str) -> String {
    let language = LanguageRegistry::with_defaults();
    let file = SourceFile::new(path, text.as_bytes().to_vec(), None);
    let analyzer = language.for_path(std::path::Path::new(path)).unwrap();
    let rows = [QueryResult::new(
        analyzer.analyze(&file).unwrap().graph,
        None,
    )];
    FormatterRegistry::with_defaults()
        .in_lang(Lang::En)
        .render_with("outline", &rows, None, None)
        .unwrap()
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
