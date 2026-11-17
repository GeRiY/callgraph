//! A megosztott props: a `HandleInertiaRequests::share` (az `Inertia\Middleware` leszármazottja) visszatérési
//! tömbjének literál kulcsai — minden oldalnak átadottak, a render-hely nem sorolja fel őket. A `return [...]` és a
//! `return array_merge(parent::share($request), [...])` alak olvasható; minden más (változó, hívás) „nem teljes”.

use super::render::{literal, split_top};
use crate::core::srcscan::{Dialect, strip_comments};
use crate::model::{Container, FileGraph, SharedFact};

/// Az Inertia-middleware osztálya-e (név szerint, vagy az `Inertia\Middleware` leszármazottja).
fn is_middleware(container: &Container) -> bool {
    container.kind == "class"
        && (container.name == "HandleInertiaRequests"
            || container
                .extends
                .iter()
                .flatten()
                .any(|parent| parent.contains("Inertia") && parent.ends_with("Middleware")))
}

/// A fájl `share` metódusának kulcsai; nincs ilyen metódus: `None`.
pub(crate) fn shared_in(graph: &FileGraph, text: &str) -> Option<SharedFact> {
    let container = graph.containers.iter().find(|c| is_middleware(c))?;
    let method = graph.methods.iter().find(|method| {
        method.name.eq_ignore_ascii_case("share")
            && graph.owns(container, method.container.as_deref(), method.line)
    })?;
    let from = usize::try_from(method.line).ok()?.checked_sub(1)?;
    let count = usize::try_from(method.end_line.unwrap_or(container.end_line))
        .ok()?
        .saturating_sub(from);
    let body = text
        .lines()
        .skip(from)
        .take(count)
        .collect::<Vec<_>>()
        .join("\n");
    Some(share_keys(&body))
}

/// A `share` törzsének (a metódus sorai) kulcsai.
pub(crate) fn share_keys(body: &str) -> SharedFact {
    let clean = strip_comments(body, Dialect::Php);
    let mut fact = SharedFact {
        keys: Vec::new(),
        complete: true,
    };
    let Some(at) = find_return(&clean) else {
        fact.complete = false;
        return fact;
    };
    let rest = &clean[at..];
    let expr = split_top(rest, ";")
        .first()
        .copied()
        .unwrap_or_default()
        .trim();
    collect(expr, &mut fact);
    fact
}

/// A `return` kulcsszó után álló szöveg helye (az utolsó `return`: a bezárt closure-ök `return`-jei nem számítanak).
fn find_return(text: &str) -> Option<usize> {
    let mut at = None;
    let mut from = 0;
    while let Some(found) = text[from..].find("return") {
        let begin = from + found;
        let end = begin + "return".len();
        let before_ok = begin == 0 || !is_word(text.as_bytes()[begin - 1]);
        let after_ok = text.as_bytes().get(end).is_none_or(|b| !is_word(*b));
        // csak a függvény-szintű (zárójelen kívüli) `return` számít: a mélyebbek closure-ökben állnak
        if before_ok && after_ok && depth_at(text, begin) <= 1 {
            at = Some(end);
        }
        from = end;
    }
    at
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// A `{` / `(` / `[` mélysége a `pos` helyen (stringeken kívül).
fn depth_at(text: &str, pos: usize) -> i32 {
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for &b in &text.as_bytes()[..pos] {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'\'' | b'"' => quote = Some(b),
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// Egy kifejezés kulcsai: tömb-literál, `array_merge(…)` vagy `parent::share(…)`; más: nem teljes.
fn collect(expr: &str, fact: &mut SharedFact) {
    let t = expr.trim();
    let lower = t.to_ascii_lowercase();
    if let Some(inner) = t.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
        array_items(inner, fact);
    } else if lower.starts_with("array(") && t.ends_with(')') {
        array_items(&t["array(".len()..t.len() - 1], fact);
    } else if lower.starts_with("array_merge(") && t.ends_with(')') {
        for arg in split_top(&t["array_merge(".len()..t.len() - 1], ",") {
            if !arg.trim().is_empty() {
                collect(arg, fact);
            }
        }
    } else if lower.replace(' ', "").starts_with("parent::share(") {
        // az alap-middleware nem oszt meg saját kulcsot
    } else {
        fact.complete = false;
    }
}

fn array_items(inner: &str, fact: &mut SharedFact) {
    for item in split_top(inner, ",") {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if let Some(spread) = item.strip_prefix("...") {
            collect(spread, fact);
            continue;
        }
        let parts = split_top(item, "=>");
        match parts.first().filter(|_| parts.len() >= 2) {
            Some(key) => match literal(key) {
                Some(key) => {
                    if !fact.keys.contains(&key) {
                        fact.keys.push(key);
                    }
                }
                None => fact.complete = false,
            },
            None => fact.complete = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::share_keys;

    #[test]
    fn merged_share_with_parent_and_literal_keys() {
        let body = "public function share(Request $request): array\n{\n    return array_merge(parent::share($request), [\n        'auth' => ['user' => fn () => $request->user()],\n        'flash' => fn () => ['ok' => 1], // c\n    ]);\n}";
        let fact = share_keys(body);
        assert_eq!(fact.keys, ["auth", "flash"]);
        assert!(fact.complete);
    }

    #[test]
    fn direct_array_spread_and_dynamic_parts() {
        let fact = share_keys("function share() { return [...parent::share($r), 'a' => 1]; }");
        assert_eq!(fact.keys, ["a"]);
        assert!(fact.complete);
        let dynamic = share_keys("function share() { return [...$extra, 'a' => 1, $k => 2]; }");
        assert!(!dynamic.complete);
        assert!(!share_keys("function share() { return $this->build(); }").complete);
    }
}
