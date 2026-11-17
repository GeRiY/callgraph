//! Az SCSS-dialektus tiszta szövegsegédei (a szkenner vezérli őket): deklaráció (`@mixin` / `@function`), import,
//! `$változó`, függvényhívások.

use crate::model::Call;

/// Egy SCSS-hívás-rekord (`@include`, `@extend`, függvény, interpoláció).
pub(super) fn call(
    container: &str,
    caller: Option<&str>,
    line: u32,
    kind: &str,
    target: &str,
    category: &str,
    args: Vec<String>,
) -> Call {
    Call::new(kind, target, category)
        .at(line)
        .within(Some(container.into()), caller.map(str::to_string))
        .with_args(Some(args))
}

/// `@mixin név(paraméterek)` / `@function név(paraméterek)` fejléc: `(fajta, név, paraméterek)`; a zárójel nélküli
/// mixin (`@mixin clearfix`) paraméter nélkül. A paraméterlista zárójel-mélység szerint tagolt (az alapértelmezett
/// érték `url(http://…)` vagy `(1, 2)` benne maradhat).
pub(super) fn declaration(header: &str) -> Option<(&'static str, String, Vec<String>)> {
    for kind in ["@mixin", "@function"] {
        let Some(rest) = header.strip_prefix(kind) else {
            continue;
        };
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim();
        let (name, params) = match rest.find('(') {
            Some(open) => {
                let close = matching_paren(rest, open).unwrap_or(rest.len());
                (rest[..open].trim(), split_top_level(&rest[open + 1..close]))
            }
            None => (rest, Vec::new()),
        };
        if name.is_empty() {
            return None;
        }
        return Some((&kind[1..], name.to_owned(), params));
    }
    None
}

/// A `(`-t záró `)` bájt-helye (idézőjeles szövegen kívül, mélység szerint).
fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    for (i, c) in text.char_indices().skip_while(|(i, _)| *i < open) {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
    }
    None
}

/// Vesszőnként tagol a legfelső szinten (zárójelen / idézőjelen kívül); a tagok vágottak, az üres kimarad.
fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut start, mut quote) = (0usize, 0usize, None);
    for (i, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' => depth += 1,
                ')' | ']' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    parts.push(text[start..i].trim().to_owned());
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    parts.push(text[start..].trim().to_owned());
    parts.retain(|part| !part.is_empty());
    parts
}

/// `név(argumentumok)` az első `(`-ig / az első `)`-ig (a beágyazott zárójelnél a külső `)` után vág).
pub(super) fn call_parts(s: &str) -> Option<(&str, Vec<String>)> {
    let open = s.find('(')?;
    let close = s[open + 1..].find(')')? + open + 1;
    Some((
        s[..open].trim(),
        s[open + 1..close]
            .split(',')
            .map(|x| x.trim().into())
            .filter(|x: &String| !x.is_empty())
            .collect(),
    ))
}

/// Az `@include` célja és argumentumai; zárójel nélkül (`@include clearfix`) a cél az első szó, argumentum nincs.
pub(super) fn include_parts(after_keyword: &str) -> Option<(&str, Vec<String>)> {
    let head = after_keyword.split('{').next().unwrap_or(after_keyword);
    if let Some(parts) = call_parts(head) {
        return Some(parts);
    }
    let target = head.split_whitespace().next()?.trim_end_matches(';');
    (!target.is_empty()).then_some((target, Vec::new()))
}

/// A szövegben lévő `név(…)` hívások `(bájt-hely, név, argumentumok)`; az interpoláció (`#{…}`) belseje kimarad.
pub(super) fn functions_at(s: &str) -> Vec<(usize, &str, Vec<String>)> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // több bájtos karakter (`á`, `ő` a megjegyzésben / sztringben) közepén nem szabad szeletelni
        if !s.is_char_boundary(i) {
            i += 1;
            continue;
        }
        if let Some(open) = s[..i].rfind("#{")
            && s[open + 2..i].find('}').is_none()
        {
            i += 1;
            continue;
        }
        if b[i].is_ascii_alphabetic() || b[i] == b'-' {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'-') {
                j += 1;
            }
            if j < b.len()
                && b[j] == b'('
                && let Some((n, a)) = call_parts(&s[i..])
            {
                out.push((i, n, a));
                i = j;
            }
        }
        i += 1;
    }
    out
}

/// Mint a `functions_at`, a helyek nélkül.
#[cfg(test)]
pub(super) fn functions(s: &str) -> Vec<(&str, Vec<String>)> {
    functions_at(s)
        .into_iter()
        .map(|(_, name, args)| (name, args))
        .collect()
}

/// `@use` / `@forward` / `@import "spec" [as alias]`: `(alias, spec)`; alias nélkül a fájlnév `_` nélkül.
pub(super) fn import(s: &str) -> Option<(String, String)> {
    let kind = s
        .strip_prefix("@use ")
        .or_else(|| s.strip_prefix("@forward "))
        .or_else(|| s.strip_prefix("@import "))?;
    let q = kind.find(['"', '\''])?;
    let tail = &kind[q + 1..];
    let end = tail.find(['"', '\'']).unwrap_or(tail.len());
    let spec = &tail[..end];
    let alias = kind[end + q + 1..]
        .split(" as ")
        .nth(1)
        .and_then(|x| x.split_whitespace().next())
        .unwrap_or_else(|| {
            spec.rsplit('/')
                .next()
                .unwrap_or(spec)
                .trim_start_matches('_')
        })
        .trim_end_matches(';');
    Some((alias.to_owned(), spec.to_owned()))
}

/// `$név: érték` utasítás: `(név, érték)`; az érték az első sor (a `;` nélkül).
pub(super) fn variable(s: &str) -> Option<(String, String)> {
    let rest = s.strip_prefix('$')?;
    let len = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .unwrap_or(rest.len());
    if len == 0 {
        return None;
    }
    let value = rest[len..].trim_start().strip_prefix(':')?;
    let value = value
        .trim()
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches(';');
    Some((format!("${}", &rest[..len]), value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regresszió: ékezetes szöveg a függvényhívás előtt (`// Fejléc háttér`, `content: 'Kérés'`) nem pánikol.
    #[test]
    fn accented_text_before_a_function_call_does_not_panic() {
        let found = functions("content: 'Kérés'; color: darken($ő-szín, 10%)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "darken");
    }

    #[test]
    fn declarations_keep_nested_parentheses_in_parameters() {
        let (kind, name, params) =
            declaration("@mixin card( $pad: 4px, $bg: url(http://x/y.png) )").unwrap();
        assert_eq!((kind, name.as_str()), ("mixin", "card"));
        assert_eq!(params, ["$pad: 4px", "$bg: url(http://x/y.png)"]);
        let (kind, name, params) = declaration("@mixin clearfix").unwrap();
        assert_eq!(
            (kind, name.as_str(), params.len()),
            ("mixin", "clearfix", 0)
        );
        assert!(declaration("@mixins x").is_none());
    }

    #[test]
    fn includes_without_parentheses_have_a_target() {
        assert_eq!(include_parts("clearfix;"), Some(("clearfix", vec![])));
        assert_eq!(
            include_parts("pad(2) {"),
            Some(("pad", vec!["2".to_owned()]))
        );
    }

    #[test]
    fn variables_and_imports_are_parsed() {
        assert_eq!(
            variable("$gap : 4px !default"),
            Some(("$gap".to_owned(), "4px !default".to_owned()))
        );
        assert_eq!(variable("$ x: 1"), None);
        assert_eq!(
            import("@use \"sass:math\" as m"),
            Some(("m".to_owned(), "sass:math".to_owned()))
        );
        assert_eq!(
            import("@use '../tokens/_colors'"),
            Some(("colors".to_owned(), "../tokens/_colors".to_owned()))
        );
    }
}
