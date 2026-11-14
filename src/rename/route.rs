//! `rename route <régi> <új>`: egy Laravel route-név átnevezése a definícióban és minden hivatkozásban.
//!
//! - **Definíció**: a `routes/**/*.php` `->name('x')` literáljai. A csoport-előtagos (`->name('profile.')->group(..)`)
//!   névből és a levél-névből áll a teljes név; ha csak a levél különbözik, a levél literálja változik. Ha az
//!   előtag része változna, csak a `*`-os alak (`'profile.*' 'account.*'`) engedett: ekkor az a literál változik,
//!   amelyikben az előtag véget ér — a csoport minden route-ja egyszerre, következetesen. Máskor elutasítjuk.
//! - **Hivatkozás** (csak pontos sztring-literál, egy- vagy kettős idézőjellel): PHP `route('x'`, `to_route('x'`,
//!   `->route('x'`, `routeIs('x'`, `Route::has('x'`, `->named('x'`; Blade `route('x'`; JS / Vue `route('x'` és
//!   `route().current('x'`. Az összefűzött / sablon-literálos hivatkozás (`route('profile.' . $x)`) csak jelzés.
//! - A route-táblával (`Laravel`) ellenőrizzük, hogy a régi név létezik és az új szabad.

use std::sync::LazyLock;

use regex::Regex;

use super::RenameRequest;
use super::php::{Kind, lex, mask, matching_paren};
use super::sites::{Plan, Site, Uncertain, line_of, line_snippet, walk};
use crate::generate::fs::Env;
use crate::generate::{GenError, GenResult};

/// A hivatkozást nyitó hívások (a nyitó zárójelig, az első argumentum előtt).
static CALL: LazyLock<Regex> = LazyLock::new(|| {
    // `signedRoute` / `temporarySignedRoute` (aláírt URL), `redirectToRoute` / `assertRedirectToRoute` (válasz,
    // teszt), `currentRouteNamed`: a `\broute` ezeket nem fogja (a `Route` szó közepén van)
    Regex::new(
        r"(?:\b(?:route|to_route|routeIs|signedRoute|temporarySignedRoute|redirectToRoute|assertRedirectToRoute|currentRouteNamed)|->\s*named|\bRoute::has|\.\s*current)\s*\(\s*",
    )
        .expect("érvényes regex")
});
/// A `name` / `as` hívás literál argumentummal (a literál tartalma az 1. vagy a 2. csoport).
static NAME_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:->|::)\s*(?:name|as)\s*\(\s*(?:'([^'\\\n]*)'|"([^"\\\n]*)")\s*\)"#)
        .expect("érvényes regex")
});

/// Egy `name('..')` literál a route-fájlban.
#[derive(Debug, Clone)]
struct Literal {
    rel: String,
    start: usize,
    end: usize,
    value: String,
    /// `->group(..)` láncban: a törzs (a `(` és a `)` helye); egyébként levél.
    group: Option<(usize, usize)>,
}

/// Az átnevezendő nevek leképezése.
struct Mapping {
    old: String,
    new: String,
    wildcard: bool,
}

impl Mapping {
    /// A név új alakja, ha érintett.
    fn map(&self, name: &str) -> Option<String> {
        if self.wildcard {
            name.strip_prefix(&self.old)
                .map(|rest| format!("{}{rest}", self.new))
        } else {
            (name == self.old).then(|| self.new.clone())
        }
    }

    /// A hivatkozás literálja: pontos név, `*`-os módban a minta (`profile.*`) is.
    fn map_literal(&self, value: &str) -> Option<String> {
        self.map(value)
    }

    fn label(&self) -> String {
        if self.wildcard {
            format!("{}* → {}*", self.old, self.new)
        } else {
            format!("{} → {}", self.old, self.new)
        }
    }
}

fn error(message: String) -> GenError {
    GenError::new(message)
}

/// A terv a route-átnevezéshez.
pub(super) fn plan(env: Env<'_>, req: &RenameRequest) -> GenResult<(String, Plan)> {
    let mapping = mapping(req)?;
    let route_files = route_files(env);
    let names = crate::packages::frameworks::laravel::route_names(&route_files);

    // a régi név létezik, az új szabad
    let existing = names.iter().filter(|n| mapping.map(n).is_some()).count();
    if existing == 0 {
        return Err(error(crate::tf!(
            "no route named {} (known names: {})",
            "nincs ilyen nevű route: {} (ismert nevek: {})",
            if mapping.wildcard {
                format!("{}*", mapping.old)
            } else {
                mapping.old.clone()
            },
            names.len()
        )));
    }
    for name in &names {
        if mapping.map(name).is_some() {
            continue;
        }
        let clash = if mapping.wildcard {
            names
                .iter()
                .filter_map(|n| mapping.map(n))
                .find(|mapped| mapped == name)
        } else {
            (name == &mapping.new).then(|| name.clone())
        };
        if let Some(taken) = clash {
            return Err(error(crate::tf!(
                "route name {} already exists, refusing the rename (nothing written)",
                "a(z) {} route-név már létezik, az átnevezést elutasítom (nem írtam semmit)",
                taken
            )));
        }
    }

    let mut plan = Plan::default();
    definitions(&route_files, &mapping, &names, &mut plan)?;
    references(env, &mapping, &mut plan);
    Ok((format!("route {}", mapping.label()), plan))
}

fn mapping(req: &RenameRequest) -> GenResult<Mapping> {
    let (old, new) = (req.old.trim(), req.new.trim());
    let wildcard = old.ends_with('*');
    if wildcard != new.ends_with('*') {
        return Err(error(
            crate::t!(
                "a wildcard rename needs `*` at the end of both names, e.g. 'profile.*' 'account.*'",
                "a * -os átnevezésnél mindkét név végén `*` kell, pl. 'profile.*' 'account.*'"
            )
            .to_owned(),
        ));
    }
    let (old, new) = if wildcard {
        (old.trim_end_matches('*'), new.trim_end_matches('*'))
    } else {
        (old, new)
    };
    let valid = |name: &str| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    if !valid(old) || !valid(new) || old == new {
        return Err(error(
            crate::t!(
                "route names must be different and consist of letters, digits, `.`, `_`, `-`",
                "a route-nevek legyenek különbözők, és csak betű, szám, `.`, `_`, `-` lehet bennük"
            )
            .to_owned(),
        ));
    }
    Ok(Mapping {
        old: old.to_owned(),
        new: new.to_owned(),
        wildcard,
    })
}

/// A `routes/**/*.php` fájlok `(rel, szöveg)` párjai.
fn route_files(env: Env<'_>) -> Vec<(String, String)> {
    walk(env.fs, &env.root.join("routes"), &["php"])
        .into_iter()
        .filter_map(|(rel, path)| {
            env.fs
                .read(&path)
                .ok()
                .map(|text| (format!("routes/{rel}"), text))
        })
        .collect()
}

/// A `->group(` törzse, ha a lánc a `from` után a `group`-ban végződik (`(`..`)` helye).
fn group_body(code: &str, from: usize) -> Option<(usize, usize)> {
    let mut p = from;
    loop {
        p += code[p..].len() - code[p..].trim_start().len();
        let rest = &code[p..];
        let after = rest.strip_prefix("->")?;
        p += 2;
        let skip = after.len() - after.trim_start().len();
        p += skip;
        let ident_len = code[p..]
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if ident_len == 0 {
            return None;
        }
        let ident = &code[p..p + ident_len];
        p += ident_len;
        p += code[p..].len() - code[p..].trim_start().len();
        if !code[p..].starts_with('(') {
            return None;
        }
        let close = matching_paren(code, p)?;
        if ident.eq_ignore_ascii_case("group") {
            return Some((p, close));
        }
        p = close + 1;
    }
}

/// Egy route-fájl `name('..')` literáljai.
fn literals(rel: &str, text: &str) -> Vec<Literal> {
    let segs = lex(text);
    let code = mask(text, &segs, |k| k != Kind::Code);
    let with_strings = mask(text, &segs, |k| {
        matches!(k, Kind::Html | Kind::Comment | Kind::Doc)
    });
    let mut out = Vec::new();
    for c in NAME_CALL.captures_iter(&with_strings) {
        let (Some(whole), Some(lit)) = (c.get(0), c.get(1).or_else(|| c.get(2))) else {
            continue;
        };
        out.push(Literal {
            rel: rel.to_owned(),
            start: lit.start(),
            end: lit.end(),
            value: lit.as_str().to_owned(),
            group: group_body(&code, whole.end()),
        });
    }
    out
}

/// A levél teljes neve és a literálok sora (a külső csoporttól a levélig).
fn chain<'a>(all: &'a [Literal], leaf: &'a Literal) -> Vec<&'a Literal> {
    let mut groups: Vec<&Literal> = all
        .iter()
        .filter(|l| {
            l.rel == leaf.rel
                && l.group
                    .is_some_and(|(open, close)| open < leaf.start && leaf.start < close)
        })
        .collect();
    groups.sort_by_key(|l| l.group.map_or(0, |(open, _)| open));
    groups.push(leaf);
    groups
}

fn definitions(
    files: &[(String, String)],
    mapping: &Mapping,
    names: &[String],
    plan: &mut Plan,
) -> GenResult<()> {
    let per_file: Vec<Vec<Literal>> = files
        .iter()
        .map(|(rel, text)| literals(rel, text))
        .collect();
    let all: Vec<Literal> = per_file.concat();
    let mut covered: Vec<String> = Vec::new();
    for leaf in all.iter().filter(|l| l.group.is_none()) {
        let seq = chain(&all, leaf);
        let full: String = seq.iter().map(|l| l.value.as_str()).collect();
        let Some(mapped) = mapping.map(&full) else {
            continue;
        };
        covered.push(full.clone());
        if mapping.wildcard {
            wildcard_site(&seq, &full, mapping, plan)?;
        } else {
            let prefix = &full[..full.len() - leaf.value.len()];
            let Some(tail) = mapped.strip_prefix(prefix) else {
                return Err(error(prefix_message(&full, &mapped)));
            };
            plan.sites
                .push(Site::replace(&leaf.rel, leaf.start, leaf.end, tail));
        }
    }
    let missing = names
        .iter()
        .find(|n| mapping.map(n).is_some() && !covered.contains(n));
    if let Some(name) = missing {
        return Err(error(crate::tf!(
            "route {} has no ->name('..') literal in routes/ (resource / generated name) — rename it by hand",
            "a(z) {} route-nak nincs ->name('..') literálja a routes/ alatt (resource / generált név) — kézzel nevezd át",
            name
        )));
    }
    Ok(())
}

/// `*`-os mód: az a literál módosul, amelyikben a régi előtag véget ér.
fn wildcard_site(
    seq: &[&Literal],
    full: &str,
    mapping: &Mapping,
    plan: &mut Plan,
) -> GenResult<()> {
    let mut before = 0usize;
    for lit in seq {
        if before + lit.value.len() >= mapping.old.len() {
            let known = &full[..before];
            let Some(head) = mapping.new.strip_prefix(known) else {
                return Err(error(crate::tf!(
                    "the enclosing group prefix {} would have to change too — rename it with its own wildcard",
                    "a befoglaló csoport előtagja ({}) is változna — nevezd át külön, saját *-gal",
                    known
                )));
            };
            let tail = &lit.value[mapping.old.len() - before..];
            plan.sites.push(Site::replace(
                &lit.rel,
                lit.start,
                lit.end,
                format!("{head}{tail}"),
            ));
            return Ok(());
        }
        before += lit.value.len();
    }
    Ok(())
}

fn prefix_message(old: &str, new: &str) -> String {
    let a = old.split('.').next().unwrap_or(old);
    let b = new.split('.').next().unwrap_or(new);
    crate::tf!(
        "{} comes from a group name prefix that would change; rename the whole group with: rename route '{}.*' '{}.*'",
        "a(z) {} csoport-előtagból áll, és az előtag változna; az egész csoport átnevezése: rename route '{}.*' '{}.*'",
        old,
        a,
        b
    )
}

/// Egy hívás első argumentuma.
enum Arg {
    /// Teljes literál: a tartalom tartománya és szövege.
    Literal(usize, usize, String),
    /// Nem teljes literál (összefűzés, sablon, interpoláció): a literál eleje.
    Partial(String),
    None,
}

/// Az argumentum a `p`-n (az idézőjelnél vagy a visszaidézőjelnél).
fn argument(text: &str, p: usize) -> Arg {
    let b = text.as_bytes();
    let Some(&quote) = b.get(p) else {
        return Arg::None;
    };
    if !matches!(quote, b'\'' | b'"' | b'`') {
        return Arg::None;
    }
    let mut j = p + 1;
    while j < b.len() && b[j] != quote {
        if b[j] == b'\\' {
            j += 1;
        }
        j += 1;
    }
    if j >= b.len() {
        return Arg::None;
    }
    let content = &text[p + 1..j];
    let interpolated = (quote == b'"' && content.contains('$'))
        || (quote == b'`' && content.contains("${"))
        || content.contains('\\');
    if interpolated {
        let cut = content.find(['$', '\\']).unwrap_or(content.len());
        return Arg::Partial(content[..cut].to_owned());
    }
    let next = text[j + 1..].trim_start();
    if next.starts_with(',') || next.starts_with(')') {
        Arg::Literal(p + 1, j, content.to_owned())
    } else {
        Arg::Partial(content.to_owned())
    }
}

/// A projekt fájljainak hivatkozásai.
fn references(env: Env<'_>, mapping: &Mapping, plan: &mut Plan) {
    let exts = ["php", "vue", "js", "ts", "jsx", "tsx", "mjs"];
    for (rel, path) in walk(env.fs, env.root, &exts) {
        let Ok(text) = env.fs.read(&path) else {
            continue;
        };
        let quick = if mapping.wildcard {
            text.contains(&mapping.old)
        } else {
            text.contains(&mapping.old)
                || mapping
                    .old
                    .split('.')
                    .next()
                    .is_some_and(|s| text.contains(s))
        };
        if !quick {
            continue;
        }
        let search = if rel.ends_with(".php") && !rel.ends_with(".blade.php") {
            let segs = lex(&text);
            mask(&text, &segs, |k| {
                matches!(k, Kind::Html | Kind::Comment | Kind::Doc)
            })
        } else {
            text.clone()
        };
        for m in CALL.find_iter(&search) {
            match argument(&search, m.end()) {
                Arg::Literal(start, end, value) => {
                    if let Some(new) = mapping.map_literal(&value) {
                        plan.sites.push(Site::replace(&rel, start, end, new));
                    }
                }
                Arg::Partial(prefix) => {
                    let related = !prefix.is_empty()
                        && (mapping.old.starts_with(&prefix) || prefix.starts_with(&mapping.old));
                    if related {
                        plan.uncertain.push(Uncertain {
                            rel: rel.clone(),
                            line: line_of(&text, m.start()),
                            text: line_snippet(&text, m.start()),
                        });
                    }
                }
                Arg::None => {}
            }
        }
    }
}
