//! A `routes` nézet route-sorai: `route     POST  orders  →  OrderController@store  name=orders.store  @  <hely>  L<sor>`.
//! Egy Laravel-lánc (`Route::middleware(..)->post(..)->name(..)`) hívásai a lánc kezdősorán, kívülről befelé
//! egymás után jönnek, és a legbelső `Route::…` statikus hívással zárulnak — egy ilyen futam egy route.

use super::{at_line, re};
use crate::model::{Call, FileGraph};

/// HTTP-ige / regisztráló hívás → a sorban megjelenő metódus.
fn verb(short: &str) -> Option<&'static str> {
    Some(match short.to_ascii_lowercase().as_str() {
        "get" => "GET",
        "post" => "POST",
        "put" => "PUT",
        "patch" => "PATCH",
        "delete" => "DELETE",
        "options" => "OPTIONS",
        "any" => "ANY",
        "match" => "MATCH",
        "resource" => "RESOURCE",
        "apiresource" => "APIRESOURCE",
        "view" => "VIEW",
        "redirect" => "REDIRECT",
        _ => return None,
    })
}

fn short(target: &str) -> &str {
    target.strip_prefix("Route::").unwrap_or(target)
}

pub(super) fn collect(g: &FileGraph, ns: &str, out: &mut Vec<String>) {
    let mut start = 0;
    for (i, c) in g.calls.iter().enumerate() {
        if c.kind == "static_call" && c.target.starts_with("Route::") {
            let run = chain(&g.calls[start..=i], c.line);
            if let Some(line) = describe(run) {
                let anchor = run
                    .iter()
                    .rev()
                    .find(|c| verb(short(&c.target)).is_some())
                    .unwrap_or(c);
                out.push(at_line("route     ", &line, ns, anchor));
            }
            start = i + 1;
        }
    }
}

/// A futam vége: a `Route::…` előtti, vele azonos soron álló láncolt (`<expr>`-en hívott) hívások.
fn chain(calls: &[Call], line: u32) -> &[Call] {
    let last = calls.len() - 1;
    let from = calls[..last]
        .iter()
        .rposition(|c| c.line != line || c.on.as_deref() != Some("<expr>"))
        .map_or(0, |p| p + 1);
    &calls[from..]
}

fn describe(run: &[Call]) -> Option<String> {
    let call = run
        .iter()
        .rev()
        .find(|c| verb(short(&c.target)).is_some())?;
    let method = verb(short(&call.target))?;
    let args = call.args.as_deref().unwrap_or_default();
    // Route::match(['get','post'], uri, action): az első argumentum a metóduslista
    let (method, rest) = match (method, args.split_first()) {
        ("MATCH", Some((list, rest))) => (methods_of(list), rest),
        (m, _) => (m.to_owned(), args),
    };
    let mut line = method;
    if let Some(uri) = rest.first() {
        line.push_str(&format!("  {}", literal(uri)));
    }
    if let Some(action) = rest.get(1) {
        line.push_str(&format!("  →  {}", handler(action)));
    }
    // a láncolt ->name()-ek (és a Route::name előtag) belülről kifelé fűzve adják a teljes nevet
    let name: String = run
        .iter()
        .rev()
        .filter(|c| short(&c.target) == "name")
        .filter_map(|c| c.args.as_deref()?.first().map(|a| literal(a)))
        .collect();
    if !name.is_empty() {
        line.push_str(&format!("  name={name}"));
    }
    Some(line)
}

fn methods_of(list: &str) -> String {
    let found: Vec<String> = re!(r#"['"](\w+)['"]"#)
        .captures_iter(list)
        .map(|c| c[1].to_ascii_uppercase())
        .collect();
    if found.is_empty() {
        "MATCH".into()
    } else {
        found.join("|")
    }
}

/// `'x'` / `"x"` → `x`; minden más (változó, kifejezés) egysorosítva, nyersen.
fn literal(arg: &str) -> String {
    let t = arg.trim();
    let quoted = t.len() >= 2
        && ((t.starts_with('\'') && t.ends_with('\'')) || (t.starts_with('"') && t.ends_with('"')));
    if quoted {
        t[1..t.len() - 1].to_owned()
    } else {
        one_line(t)
    }
}

/// A route-kezelő: `[X::class, 'm']` / `'X@m'` → `X@m`, `X::class` → `X`, closure → `closure`.
fn handler(arg: &str) -> String {
    let t = arg.trim();
    if let Some(c) =
        re!(r#"^\[\s*\\?([\w\\]+)::class\s*(?:,\s*['"](\w+)['"]\s*)?,?\s*\]$"#).captures(t)
    {
        return match c.get(2) {
            Some(m) => format!("{}@{}", &c[1], m.as_str()),
            None => c[1].to_owned(),
        };
    }
    if let Some(c) = re!(r"^\\?([\w\\]+)::class$").captures(t) {
        return c[1].to_owned();
    }
    if re!(r"^(static\s+)?(function|fn)\b").is_match(t) {
        return "closure".into();
    }
    literal(t)
}

fn one_line(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(60) {
        Some((i, _)) => format!("{}…", &flat[..i]),
        None => flat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_forms() {
        assert_eq!(
            handler("[OrderController::class, 'store']"),
            "OrderController@store"
        );
        assert_eq!(handler("[\\App\\X::class]"), "App\\X");
        assert_eq!(handler("'OrderController@show'"), "OrderController@show");
        assert_eq!(handler("CikkController::class"), "CikkController");
        assert_eq!(handler("function () {\n return 1;\n}"), "closure");
        assert_eq!(handler("fn() => 1"), "closure");
    }

    #[test]
    fn literal_and_methods() {
        assert_eq!(literal("'orders'"), "orders");
        assert_eq!(literal("$uri"), "$uri");
        assert_eq!(methods_of("['get', 'post']"), "GET|POST");
    }
}
