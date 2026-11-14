//! A PHP-oldali validáció kiolvasása a forrásból: egy controller-metódus paraméterei (FormRequest-típus), a
//! `rules()` tömb kulcsai és szabály-tokenjei, a metódus-törzs inline validációi (`$request->validate([..])`,
//! `$this->validate($request, [..])`, `Validator::make($x, [..])`) és az explicit mező-olvasások
//! (`$request->boolean('x')`). Szövegből dolgozik (a szűkített gráfban nincs törzs / argumentum), a megjegyzések
//! előzetes lecserélésével. Ami nem értelmezhető, kimarad — soha nem hiba.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

use super::super::inertia::render::split_top;
use super::routes::{matching, strip_comments};

/// Egy szabály-kulcs tokenei, amelyek az összevetést érdeklik.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Flags {
    pub required: bool,
    pub nullable: bool,
    pub sometimes: bool,
    pub confirmed: bool,
}

impl Flags {
    fn merge(&mut self, other: Self) {
        self.required |= other.required;
        self.nullable |= other.nullable;
        self.sometimes |= other.sometimes;
        self.confirmed |= other.confirmed;
    }
}

/// Egy akció validációja: a szabály-kulcsok (felső szintű kulccsal), az explicit olvasások, és hogy találtunk-e
/// egyáltalán szabály-forrást.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Validation {
    pub rules: BTreeMap<String, Flags>,
    pub reads: BTreeSet<String>,
    pub found: bool,
}

impl Validation {
    /// Egy szabály-tömb (a `[..]` belseje) beolvasása.
    fn add_rules(&mut self, inner: &str) {
        self.found = true;
        for entry in split_top(inner, ",") {
            let parts = split_top(entry, "=>");
            if parts.len() < 2 {
                continue;
            }
            let Some(key) = literal_key(parts[0]) else {
                continue;
            };
            let (top, nested) = match key.split_once('.') {
                Some((top, _)) => (top.to_owned(), true),
                None => (key, false),
            };
            let flags = flags_of(parts[1]);
            let slot = self.rules.entry(top).or_default();
            // az `items.*.name` szabálya nem teszi kötelezővé az `items`-t: csak a létezését jelzi
            if !nested {
                slot.merge(flags);
            }
        }
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.found |= other.found;
        self.reads.extend(other.reads);
        for (key, flags) in other.rules {
            self.rules.entry(key).or_default().merge(flags);
        }
    }
}

/// A fájl `use` importjai (alias → teljes név) és a névtere.
pub(super) fn imports_of(text: &str) -> (BTreeMap<String, String>, Option<String>) {
    static USE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^\s*use\s+([\\\w]+)(?:\s+as\s+(\w+))?\s*;").expect("valid regex")
    });
    static NAMESPACE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^\s*namespace\s+([\\\w]+)\s*;").expect("valid regex"));
    let imports = USE
        .captures_iter(text)
        .map(|c| {
            let full = c[1].trim_start_matches('\\').to_owned();
            let alias = c.get(2).map_or_else(
                || full.rsplit('\\').next().unwrap_or(&full).to_owned(),
                |a| a.as_str().to_owned(),
            );
            (alias, full)
        })
        .collect();
    let namespace = NAMESPACE.captures(text).map(|c| c[1].to_owned());
    (imports, namespace)
}

/// Egy típus teljes neve a fájl importjai / névtere szerint; `None`: nem osztály-típus (skalár, unió, üres).
pub(super) fn qualify(
    ty: &str,
    imports: &BTreeMap<String, String>,
    namespace: Option<&str>,
) -> Option<String> {
    let ty = ty.trim().trim_start_matches('?');
    if ty.is_empty() || ty.contains('|') || ty.contains('&') {
        return None;
    }
    if let Some(rest) = ty.strip_prefix('\\') {
        return Some(rest.to_owned());
    }
    let (first, tail) = match ty.split_once('\\') {
        Some((first, tail)) => (first, Some(tail)),
        None => (ty, None),
    };
    if let Some(full) = imports.get(first) {
        return Some(match tail {
            Some(tail) => format!("{full}\\{tail}"),
            None => full.clone(),
        });
    }
    Some(match namespace.filter(|ns| !ns.is_empty()) {
        Some(ns) => format!("{ns}\\{ty}"),
        None => ty.to_owned(),
    })
}

/// A tisztított szövegből a metódus (paraméter-lista, törzs); nincs ilyen metódus / törzs: `None`.
pub(super) fn method_parts(clean: &str, name: &str) -> Option<(String, String)> {
    let pattern = format!(r"(?i)\bfunction\s+&?\s*{}\s*\(", regex::escape(name));
    let found = Regex::new(&pattern).ok()?.find(clean)?;
    let open = found.end() - 1;
    let close = matching(clean, open, clean.len())?;
    let params = clean.get(open + 1..close)?.to_owned();
    let brace = close + clean.get(close..)?.find(['{', ';'])?;
    if clean.as_bytes().get(brace) != Some(&b'{') {
        return None;
    }
    let end = matching(clean, brace, clean.len())?;
    Some((params, clean.get(brace + 1..end)?.to_owned()))
}

/// A paraméterek osztály-típusai (a változó előtti típus-token), a sorrendjükben.
pub(super) fn param_types(params: &str) -> Vec<String> {
    split_top(params, ",")
        .into_iter()
        .filter_map(|param| {
            let tokens: Vec<&str> = param.split_whitespace().collect();
            let var = tokens
                .iter()
                .position(|t| t.trim_start_matches(['&', '.']).starts_with('$'))?;
            let ty = tokens.get(var.checked_sub(1)?)?;
            Some((*ty).to_owned())
        })
        .collect()
}

/// A FormRequest-osztály `rules()` metódusa(i) + az explicit olvasásai (`$this->boolean('remember')`).
pub(super) fn request_validation(text: &str) -> Validation {
    let clean = strip_comments(text);
    let mut out = Validation::default();
    if let Some((_, body)) = method_parts(&clean, "rules") {
        static RETURN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?i)\breturn\s*(\[|array\s*\()").expect("valid regex"));
        static ASSIGN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?i)\$\w+\s*=\s*(\[|array\s*\()").expect("valid regex"));
        let mut arrays: Vec<String> = RETURN
            .captures_iter(&body)
            .filter_map(|c| array_inner(&body, c.get(0)?.end() - 1))
            .collect();
        if arrays.is_empty() {
            arrays = ASSIGN
                .captures_iter(&body)
                .filter_map(|c| array_inner(&body, c.get(0)?.end() - 1))
                .take(1)
                .collect();
        }
        for inner in arrays {
            out.add_rules(&inner);
        }
    }
    out.reads = reads(&clean);
    out
}

/// A metódus-törzs inline validációi + explicit olvasásai.
pub(super) fn body_validation(body: &str) -> Validation {
    static CALL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:->\s*validate(?:WithBag)?|Validator\s*::\s*make|\bvalidator)\s*\(")
            .expect("valid regex")
    });
    let mut out = Validation::default();
    for found in CALL.find_iter(body) {
        let open = found.end() - 1;
        let Some(close) = matching(body, open, body.len()) else {
            continue;
        };
        let Some(inner) = body.get(open + 1..close) else {
            continue;
        };
        let args = split_top(inner, ",");
        let text = found.as_str();
        let rules_arg = if text.contains("make") || text.starts_with("validator") {
            args.get(1).copied()
        } else {
            args.iter()
                .copied()
                .find(|a| is_array_start(a))
                .or_else(|| {
                    // `->validate($rules)` — a változó értéke a törzsben
                    args.iter().copied().find(|a| a.trim().starts_with('$'))
                })
        };
        if let Some(inner) = rules_arg.and_then(|arg| array_value(body, arg)) {
            out.add_rules(&inner);
        }
    }
    out.reads = reads(body);
    out
}

fn is_array_start(arg: &str) -> bool {
    let t = arg.trim_start();
    t.starts_with('[') || t.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("array"))
}

/// Egy argumentum tömb-literál belseje, vagy ha `$változó`, a törzsbeli első értékadása.
fn array_value(body: &str, arg: &str) -> Option<String> {
    let t = arg.trim();
    if is_array_start(t) {
        let start = if t.starts_with('[') { 0 } else { t.find('(')? };
        return array_inner(t, start);
    }
    let var = t
        .strip_prefix('$')
        .filter(|v| v.chars().all(|c| c.is_alphanumeric() || c == '_'))?;
    let pattern = format!(r"(?i)\${}\s*=\s*(\[|array\s*\()", regex::escape(var));
    let found = Regex::new(&pattern).ok()?.captures(body)?;
    array_inner(body, found.get(0)?.end() - 1)
}

/// A `at` helyi nyitó jel (`[` vagy `(`) párjának belseje.
fn array_inner(text: &str, at: usize) -> Option<String> {
    let close = matching(text, at, text.len())?;
    Some(text.get(at + 1..close)?.to_owned())
}

/// Egy kulcs sima string-literál alakja.
fn literal_key(text: &str) -> Option<String> {
    let t = text.trim();
    let quote = t.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = t.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inner.is_empty() && !inner.contains(quote)).then(|| inner.to_owned())
}

/// A szabály-érték tokenjei: az érték minden idézett stringje `|` mentén, a `:` előtti fejjel.
fn flags_of(value: &str) -> Flags {
    static QUOTED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"['"]([^'"]+)['"]"#).expect("valid regex"));
    let mut flags = Flags::default();
    for quoted in QUOTED.captures_iter(value) {
        for token in quoted[1].split('|') {
            match token.split(':').next().unwrap_or("").trim() {
                "required" => flags.required = true,
                "nullable" => flags.nullable = true,
                "sometimes" => flags.sometimes = true,
                "confirmed" => flags.confirmed = true,
                _ => {}
            }
        }
    }
    flags
}

/// Az explicit mező-olvasások felső szintű kulcsai: `$request->boolean('x')` / `input` / `get` / `has` / `filled` …,
/// `->only([..])`, és a `$request->mező` tulajdonság-elérés.
pub(super) fn reads(text: &str) -> BTreeSet<String> {
    static CALL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?:\$request|\$this|\$req|request\(\))\s*->\s*(?:boolean|input|get|has|filled|string|integer|float|date|str|query|post|exists|missing|enum|whenHas|whenFilled|hasAny)\(\s*['"]([\w.\-]+)['"]"#,
        )
        .expect("valid regex")
    });
    static ONLY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:\$request|\$this|\$req|request\(\))\s*->\s*only\s*\(([^)]*)\)")
            .expect("valid regex")
    });
    static PROPERTY: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\$(?:request|req)\s*->\s*([A-Za-z_]\w*)(\s*\()?").expect("valid regex")
    });
    static QUOTED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"['"]([\w.\-]+)['"]"#).expect("valid regex"));
    let top = |name: &str| name.split('.').next().unwrap_or(name).to_owned();
    let mut out = BTreeSet::new();
    for c in CALL.captures_iter(text) {
        out.insert(top(&c[1]));
    }
    for c in ONLY.captures_iter(text) {
        for q in QUOTED.captures_iter(&c[1]) {
            out.insert(top(&q[1]));
        }
    }
    for c in PROPERTY.captures_iter(text) {
        if c.get(2).is_none() {
            out.insert(c[1].to_owned());
        }
    }
    out
}
