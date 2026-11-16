//! A CSS-osztály ↔ használat olvasói: a stíluslap szelektoraiból a definiált osztályok (egyszerű `.név`, a SCSS
//! `&-suffix` összefűzéssel; a kulcskeret-, mixin- és `@theme` blokkok kimaradnak), a sablonokból / szkriptekből a
//! használatok (`class="…"`, `:class="{…}"`, `clsx(…)`, `classList.add('x')` — minden string-literál szava),
//! és a dinamikus előtagok / utótagok (`'btn-' + x`, `` `btn-${x}` ``, `btn-{{ $x }}`).
//! Hibatűrő: ami nem olvasható, kimarad.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::core::LineIndex;

/// Egy osztály használati helyeinek felső határa (a „hol használják” sorhoz elég).
const MAX_LOCATIONS: usize = 50;

/// A használatok gyűjtése: osztály → helyek (fájl-index, sor), dinamikus előtagok / utótagok.
#[derive(Debug, Default)]
pub(crate) struct Uses {
    pub(crate) words: HashMap<String, Vec<(u32, u32)>>,
    pub(crate) prefixes: HashSet<String>,
    pub(crate) suffixes: HashSet<String>,
}

fn re(cell: &'static LazyLock<Regex>) -> &'static Regex {
    cell
}

fn comment_banner() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?is)\A\s*/\*!?[^*]{0,200}(bootstrap|font ?awesome|normalize|animate\.css|tailwindcss v|swiper|select2|jquery|fullcalendar|bulma)",
        )
        .expect("érvényes regex")
    });
    re(&RE)
}

fn class_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\.(-?[_a-zA-Z][\w-]*)").expect("érvényes regex"));
    re(&RE)
}

fn suffix_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&([\w-]+)").expect("érvényes regex"));
    re(&RE)
}

fn attr_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?:\bclass|\bclassName|\bclassNames|:class|v-bind:class|\[ngClass\]|\bngClass)\s*=\s*(?:"([^"]{0,2000})"|'([^']{0,2000})')"#,
        )
        .expect("érvényes regex")
    });
    re(&RE)
}

fn literal_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"'([^'\\\n]{1,300})'|"([^"\\\n]{1,300})"|`([^`]{1,600})`"#)
            .expect("érvényes regex")
    });
    re(&RE)
}

fn module_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?:\bstyles?|\bclasses|\bcss|\$style|\bcls)(?:\.([A-Za-z_][\w-]*)|\[\s*['"]([\w-]+)['"]\s*\])"#,
        )
        .expect("érvényes regex")
    });
    re(&RE)
}

fn interpolation_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)\{\{.*?\}\}|\{!!.*?!!\}|<\?.*?\?>|\$\{[^}]*\}|\{\$[^}]*\}|\$[A-Za-z_]\w*")
            .expect("érvényes regex")
    });
    re(&RE)
}

fn word_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[\w\x{2}-]+").expect("érvényes regex"));
    re(&RE)
}

fn apply_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"@(?:apply|extend)\s+([^;}{]+)").expect("érvényes regex"));
    re(&RE)
}

/// Minifikált / harmadik féltől származó stíluslap-e (a saját szabályok nem ilyenek).
pub(super) fn looks_vendor(rel: &str, text: &str) -> bool {
    let rooted = format!("/{rel}");
    if rel.ends_with(".min.css") || rooted.contains("/dist/") || rooted.contains("/build/") {
        return true;
    }
    if text.lines().take(50).any(|line| line.len() > 2000) {
        return true;
    }
    comment_banner().is_match(text)
}

/// A stíluslap definiált osztályai (név, sor); az egyszerű osztály-szelektorok, a beágyazott `&-x` összefűzéssel.
pub(super) fn definitions(text: &str) -> Vec<(String, u32)> {
    struct Frame {
        classes: Vec<String>,
        skip: bool,
    }
    let b = text.as_bytes();
    let n = b.len();
    let mut out: Vec<(String, u32)> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut prelude: Vec<u8> = Vec::new();
    let mut line = 1u32;
    let mut pre_line = 1u32;
    let mut i = 0;
    while i < n {
        let c = b[i];
        match c {
            b'\n' => {
                line += 1;
                if !prelude.is_empty() {
                    prelude.push(b' ');
                }
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..].find("*/").map_or(n, |at| i + 2 + at + 2);
                line += text[i..end].matches('\n').count() as u32;
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'/')
                && (i == 0
                    || b[i - 1].is_ascii_whitespace()
                    || matches!(b[i - 1], b';' | b'{' | b'}')) =>
            {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' | b'\'' => {
                let start = i;
                i += 1;
                while i < n && b[i] != c && b[i] != b'\n' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i = (i + 1).min(n);
                line += text[start..i].matches('\n').count() as u32;
                prelude.extend_from_slice(b"_");
            }
            b'\\' => {
                prelude.push(1);
                i += 2;
            }
            b'#' if b.get(i + 1) == Some(&b'{') => {
                // SCSS interpoláció: `#{...}` kihagyva
                let mut depth = 0i32;
                while i < n {
                    match b[i] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        b'\n' => line += 1,
                        _ => {}
                    }
                    i += 1;
                }
                prelude.extend_from_slice(b"_");
            }
            b'{' => {
                let text_prelude = String::from_utf8_lossy(&prelude).trim().to_owned();
                let parent_skip = stack.last().is_some_and(|frame| frame.skip);
                let parent_classes = stack
                    .last()
                    .map(|frame| frame.classes.clone())
                    .unwrap_or_default();
                let frame = if parent_skip {
                    Frame {
                        classes: Vec::new(),
                        skip: true,
                    }
                } else if let Some(at_rule) = text_prelude.strip_prefix('@') {
                    let name: String = at_rule
                        .chars()
                        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
                        .collect();
                    match name.as_str() {
                        "media" | "supports" | "layer" | "container" | "document" | "scope"
                        | "starting-style" | "include" | "at-root" => Frame {
                            classes: parent_classes,
                            skip: false,
                        },
                        _ => Frame {
                            classes: Vec::new(),
                            skip: true,
                        },
                    }
                } else {
                    let classes = selector_classes(&text_prelude, &parent_classes);
                    for class in &classes {
                        out.push((class.clone(), pre_line));
                    }
                    Frame {
                        classes: if classes.is_empty() {
                            parent_classes
                        } else {
                            classes
                        },
                        skip: false,
                    }
                };
                stack.push(frame);
                prelude.clear();
                i += 1;
            }
            b'}' => {
                stack.pop();
                prelude.clear();
                i += 1;
            }
            b';' => {
                prelude.clear();
                i += 1;
            }
            _ => {
                if prelude.is_empty() && !c.is_ascii_whitespace() {
                    pre_line = line;
                }
                if !(prelude.is_empty() && c.is_ascii_whitespace()) {
                    prelude.push(c);
                }
                i += 1;
            }
        }
    }
    out
}

/// A szelektor-lista osztályai; a `&-x` / `&__x` a szülő osztályaihoz fűződik. A zárójeles (`:not(.x)`, `:is(…)`)
/// és az attribútum-szelektor (`[class*=x]`) tartalma hivatkozás, nem definíció; az escape-elt (`.md\:flex`) név
/// utility, kimarad.
fn selector_classes(selector: &str, parents: &[String]) -> Vec<String> {
    static ATTR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\[[^\]]*\]").expect("érvényes regex"));
    static PARENS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\([^()]*\)").expect("érvényes regex"));
    let mut text = ATTR.replace_all(selector, "").into_owned();
    for _ in 0..4 {
        let next = PARENS.replace_all(&text, "").into_owned();
        if next == text {
            break;
        }
        text = next;
    }
    let mut found = Vec::new();
    for part in text.split(',') {
        for caps in class_re().captures_iter(part) {
            let (Some(whole), Some(name)) = (caps.get(0), caps.get(1)) else {
                continue;
            };
            if part[whole.end()..].starts_with('\u{1}') {
                continue;
            }
            found.push(name.as_str().to_owned());
        }
        for caps in suffix_re().captures_iter(part) {
            let Some(suffix) = caps.get(1) else { continue };
            for parent in parents {
                found.push(format!("{parent}{}", suffix.as_str()));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// A stíluslap `@apply` / `@extend` szavai (felhasznált osztályok).
pub(super) fn applied_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for caps in apply_re().captures_iter(text) {
        if let Some(list) = caps.get(1) {
            for word in list.as_str().split_whitespace() {
                out.push(
                    word.trim_start_matches(['.', '%'])
                        .trim_end_matches("!important")
                        .to_owned(),
                );
            }
        }
    }
    out
}

/// Egy sablon / szkript osztály-használatai: a literálok szavai (a `defined` osztályokra), az attribútum-értékek,
/// a CSS-modul hivatkozások, és a dinamikus előtagok / utótagok.
pub(super) fn scan_uses(text: &str, file: u32, defined: &HashSet<String>, uses: &mut Uses) {
    let mut lines: Option<LineIndex> = None;
    let mut line_of = |offset: usize| -> u32 {
        lines
            .get_or_insert_with(|| LineIndex::new(text))
            .line_of(offset)
    };
    for caps in attr_re().captures_iter(text) {
        if let (Some(whole), Some(value)) = (caps.get(0), caps.get(1).or_else(|| caps.get(2))) {
            let at = line_of(whole.start());
            scan_value(value.as_str(), at, file, false, false, defined, uses);
        }
    }
    for caps in literal_re().captures_iter(text) {
        let (Some(whole), Some(value)) = (
            caps.get(0),
            caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3)),
        ) else {
            continue;
        };
        let before = text[..whole.start()].trim_end().chars().next_back();
        let after = text[whole.end()..].trim_start().chars().next();
        let at = line_of(whole.start());
        scan_value(
            value.as_str(),
            at,
            file,
            before == Some('+'),
            after == Some('+'),
            defined,
            uses,
        );
    }
    for caps in module_re().captures_iter(text) {
        let (Some(whole), Some(name)) = (caps.get(0), caps.get(1).or_else(|| caps.get(2))) else {
            continue;
        };
        if defined.contains(name.as_str()) {
            let at = line_of(whole.start());
            record(uses, name.as_str(), file, at);
        }
    }
}

fn record(uses: &mut Uses, word: &str, file: u32, line: u32) {
    let places = uses.words.entry(word.to_owned()).or_default();
    if places.len() < MAX_LOCATIONS && places.last() != Some(&(file, line)) {
        places.push((file, line));
    }
}

/// Egy literál / attribútum-érték szavai. Az interpoláció (`{{ }}`, `${}`, `<?php ?>`, `$x`) helyén `\u{2}` jelöl:
/// az előtte / utána álló szórészlet dinamikus előtag / utótag.
fn scan_value(
    value: &str,
    line: u32,
    file: u32,
    plus_before: bool,
    plus_after: bool,
    defined: &HashSet<String>,
    uses: &mut Uses,
) {
    let replaced = interpolation_re().replace_all(value, "\u{2}");
    for run in word_re().find_iter(&replaced) {
        let text = run.as_str();
        if text.contains('\u{2}') {
            let parts: Vec<&str> = text.split('\u{2}').collect();
            if let Some(first) = parts
                .first()
                .filter(|first| first.len() >= 2 && !text.starts_with('\u{2}'))
            {
                uses.prefixes.insert((*first).to_owned());
            }
            if let Some(last) = parts
                .last()
                .filter(|last| last.len() >= 2 && !text.ends_with('\u{2}'))
            {
                uses.suffixes.insert((*last).to_owned());
            }
            continue;
        }
        if defined.contains(text) {
            record(uses, text, file, line);
        }
        // `'btn-' + x` / `x + '-active'`: a literál széle dinamikus
        if text.len() >= 2 {
            if plus_after && run.end() == replaced.len() {
                uses.prefixes.insert(text.to_owned());
            }
            if plus_before && run.start() == 0 {
                uses.suffixes.insert(text.to_owned());
            }
        }
    }
}
