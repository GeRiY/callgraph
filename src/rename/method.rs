//! `rename method <Osztály>::<régi> <új>`: egy PHP-metódus átnevezése a deklarációban és a hívásokban.
//!
//! - **Érintett osztályok**: a megnevezett osztály, a leszármazottai (felülírások), interfésznél az implementálók.
//!   Ha a metódust egy ős vagy interfész is deklarálja, elutasítjuk (ott kell átnevezni, különben elromlana a
//!   felülírás).
//! - **Hívás**: a projekt-elemző (`FileGraph.calls`) `resolved` mezője dönt: `$x->m(` / `?->m(` / `Pay::m(` /
//!   `self::` / `static::` / `parent::` / `$this->m(`. A fogadó típusa ismeretlen (`resolved` nincs) → csak jelzés.
//! - **Callable tömb és route-akció**: `[Pay::class, 'm']`, `[PayController::class, 'm']`, `[$this, 'm']`.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use super::RenameRequest;
use super::php::{Decl, PhpFile, find_decl, load_all};
use super::sites::{Plan, Site, Uncertain};
use crate::generate::fs::Env;
use crate::generate::{Emitter, GenError, GenResult, PhpEmitter};

/// Az osztályok szülő-térképe (kisbetűs teljes nevekkel).
struct Hierarchy {
    parents: BTreeMap<String, Vec<String>>,
}

impl Hierarchy {
    fn new(files: &[PhpFile]) -> Self {
        let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for decl in files.iter().flat_map(|f| f.decls.iter()) {
            parents
                .entry(decl.fqcn.to_ascii_lowercase())
                .or_default()
                .extend(decl.parents.iter().map(|p| p.to_ascii_lowercase()));
        }
        Self { parents }
    }

    /// Az osztály és minden leszármazottja / implementálója.
    fn family(&self, root: &str) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        set.insert(root.to_owned());
        loop {
            let added: Vec<String> = self
                .parents
                .iter()
                .filter(|(child, parents)| {
                    !set.contains(*child) && parents.iter().any(|p| set.contains(p))
                })
                .map(|(child, _)| child.clone())
                .collect();
            if added.is_empty() {
                return set;
            }
            set.extend(added);
        }
    }

    /// Az osztály összes őse és interfésze (a gyökér nélkül).
    fn ancestors(&self, root: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut todo = vec![root.to_owned()];
        while let Some(current) = todo.pop() {
            for parent in self.parents.get(&current).into_iter().flatten() {
                if seen.insert(parent.clone()) {
                    todo.push(parent.clone());
                }
            }
        }
        seen
    }
}

/// A `function név(` deklarációk a típus törzsében: a név bájt-tartományai.
fn declarations(file: &PhpFile, decl: &Decl, name: &str) -> Vec<(usize, usize)> {
    let pattern = format!(r"(?i)\bfunction\s+&?\s*({})\s*\(", regex::escape(name));
    let Ok(re) = Regex::new(&pattern) else {
        return Vec::new();
    };
    let body = &file.code[decl.open..decl.close];
    re.captures_iter(body)
        .filter_map(|c| c.get(1))
        .map(|m| (decl.open + m.start(), decl.open + m.end()))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Vote {
    Rename,
    Skip,
    Unknown,
}

/// Egy előfordulás szavazatai: a pontosak (a hívásnak egyetlen jelöltje volt) és a megosztottak.
#[derive(Default)]
struct Ballot {
    end: usize,
    precise: Vec<Vote>,
    shared: Vec<Vote>,
}

/// A terv a metódus-átnevezéshez.
pub(super) fn plan(env: Env<'_>, req: &RenameRequest) -> GenResult<(String, Plan)> {
    let usage = || {
        GenError::new(
            crate::t!(
                "give the method as Class::method, e.g. rename method 'App\\Services\\Pay::charge' bill",
                "a metódust Osztály::metódus alakban add meg, pl. rename method 'App\\Services\\Pay::charge' bill"
            )
            .to_owned(),
        )
    };
    let (class, method) = req.old.trim().rsplit_once("::").ok_or_else(usage)?;
    let new = req.new.trim();
    let valid = |name: &str| {
        let mut chars = name.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if !valid(method) || !valid(new) || method == new {
        return Err(usage());
    }

    let files = load_all(env.fs, env.root);
    let (target_file, target_decl) = find_decl(&files, class).map_err(GenError::new)?;
    let hierarchy = Hierarchy::new(&files);
    let target = target_decl.fqcn.to_ascii_lowercase();
    if declarations(target_file, target_decl, method).is_empty() {
        return Err(GenError::new(crate::tf!(
            "{} has no method {} of its own (inherited methods: rename them where they are declared)",
            "a(z) {} osztálynak nincs saját {} metódusa (örökölt metódust ott nevezz át, ahol deklarálva van)",
            target_decl.fqcn,
            method
        )));
    }
    let ancestors = hierarchy.ancestors(&target);
    for (file, decl) in files
        .iter()
        .flat_map(|f| f.decls.iter().map(move |d| (f, d)))
    {
        if ancestors.contains(&decl.fqcn.to_ascii_lowercase())
            && !declarations(file, decl, method).is_empty()
        {
            return Err(GenError::new(crate::tf!(
                "{}::{} is also declared by {} — rename it there (an override would break)",
                "a(z) {}::{} metódust a(z) {} is deklarálja — ott nevezd át (a felülírás elromlana)",
                target_decl.fqcn,
                method,
                decl.fqcn
            )));
        }
    }
    let family = hierarchy.family(&target);

    let mut plan = Plan::default();
    for file in &files {
        for decl in file
            .decls
            .iter()
            .filter(|d| family.contains(&d.fqcn.to_ascii_lowercase()))
        {
            if !declarations(file, decl, new).is_empty() {
                return Err(GenError::new(crate::tf!(
                    "{} already has a method {} — refusing the rename (nothing written)",
                    "a(z) {} osztálynak már van {} metódusa — az átnevezést elutasítom (nem írtam semmit)",
                    decl.fqcn,
                    new
                )));
            }
            for (start, end) in declarations(file, decl, method) {
                plan.sites.push(Site::replace(&file.rel, start, end, new));
            }
        }
    }

    let lower = method.to_ascii_lowercase();
    for file in &files {
        if !file.text.to_ascii_lowercase().contains(&lower) {
            continue;
        }
        calls(file, method, new, &family, &mut plan);
        callables(file, method, new, &family, &mut plan);
    }
    Ok((
        format!("method {}::{method} → {new}", target_decl.fqcn),
        plan,
    ))
}

/// Az osztály, amelyhez a hívás tartozik; `None`: ismeretlen fogadó.
fn call_class(file: &PhpFile, call: &crate::model::Call, line_offset: usize) -> Option<String> {
    let prefix = call
        .target
        .rsplit_once("::")
        .map(|(c, _)| c.trim_start_matches('\\'));
    let resolved = call
        .resolved
        .as_deref()
        .and_then(|r| r.rsplit_once("::"))
        .map(|(c, _)| c.trim_start_matches('\\'));
    let name = prefix
        .filter(|p| ["self", "static", "parent"].contains(&p.to_ascii_lowercase().as_str()))
        .or(resolved);
    match name {
        Some(name) => relative_class(file, name, line_offset),
        // `$this->m()` feloldott típus nélkül: a környező osztály
        None if call.on.as_deref() == Some("this") => relative_class(file, "self", line_offset),
        None => None,
    }
}

/// `self` / `static` / `parent` feloldása a környező deklarációból, egyébként a név maga.
fn relative_class(file: &PhpFile, name: &str, offset: usize) -> Option<String> {
    match name.to_ascii_lowercase().as_str() {
        "self" | "static" => file.enclosing(offset).map(|d| d.fqcn.clone()),
        "parent" => file.enclosing(offset).and_then(|d| d.extends.clone()),
        _ => Some(name.to_owned()),
    }
}

/// A gráf hívásai: szavazás a forrásbeli előfordulásokra.
fn calls(file: &PhpFile, method: &str, new: &str, family: &BTreeSet<String>, plan: &mut Plan) {
    let graph = PhpEmitter.analyze(&file.rel, &file.text);
    let escaped = regex::escape(method);
    let (Ok(member), Ok(statics)) = (
        Regex::new(&format!(r"(?i)(?:->|\?->)\s*({escaped})\s*\(")),
        Regex::new(&format!(r"(?i)::\s*({escaped})\s*\(")),
    ) else {
        return;
    };
    let mut ballots: BTreeMap<usize, Ballot> = BTreeMap::new();
    for call in &graph.calls {
        let is_static = call.kind == "static_call";
        if !is_static && call.kind != "method_call" && call.kind != "nullsafe_method_call" {
            continue;
        }
        let name = if is_static {
            call.target.rsplit_once("::").map_or("", |(_, m)| m)
        } else {
            call.target.as_str()
        };
        if !name.eq_ignore_ascii_case(method) {
            continue;
        }
        let first = call.line as usize;
        let last = call.end_line.map_or(first, |l| l as usize).max(first);
        let from = file.line_start(first);
        let to = file.line_start(last + 1);
        let vote = match call_class(file, call, from) {
            Some(class) if family.contains(&class.to_ascii_lowercase()) => Vote::Rename,
            Some(_) => Vote::Skip,
            None => Vote::Unknown,
        };
        let re = if is_static { &statics } else { &member };
        let found: Vec<(usize, usize)> = re
            .captures_iter(&file.code[from..to])
            .filter_map(|c| c.get(1))
            .map(|m| (from + m.start(), from + m.end()))
            .collect();
        if found.is_empty() {
            if vote == Vote::Rename {
                plan.uncertain.push(Uncertain {
                    rel: file.rel.clone(),
                    line: first,
                    text: file.line_text(from),
                });
            }
            continue;
        }
        let precise = found.len() == 1;
        for (start, end) in found {
            let ballot = ballots.entry(start).or_default();
            ballot.end = end;
            if precise {
                ballot.precise.push(vote);
            } else {
                ballot.shared.push(vote);
            }
        }
    }
    for (start, ballot) in ballots {
        let votes = if ballot.precise.is_empty() {
            &ballot.shared
        } else {
            &ballot.precise
        };
        if votes.iter().all(|v| *v == Vote::Skip) {
            continue;
        }
        if ballot.precise.is_empty() || !votes.iter().all(|v| *v == Vote::Rename) {
            plan.uncertain.push(Uncertain {
                rel: file.rel.clone(),
                line: file.line_of(start),
                text: file.line_text(start),
            });
        } else {
            plan.sites
                .push(Site::replace(&file.rel, start, ballot.end, new));
        }
    }
}

/// `[Pay::class, 'charge']`, `[$this, 'charge']` (route-akció és callable tömb).
fn callables(file: &PhpFile, method: &str, new: &str, family: &BTreeSet<String>, plan: &mut Plan) {
    let escaped = regex::escape(method);
    let pattern = format!(
        r#"(?i)\[\s*(\$this|\\?[A-Za-z_][A-Za-z0-9_\\]*::class)\s*,\s*(?:'({escaped})'|"({escaped})")\s*\]"#
    );
    let Ok(re) = Regex::new(&pattern) else {
        return;
    };
    for c in re.captures_iter(&file.code_strings) {
        let (Some(whole), Some(receiver), Some(literal)) =
            (c.get(0), c.get(1), c.get(2).or_else(|| c.get(3)))
        else {
            continue;
        };
        let class = if receiver.as_str().eq_ignore_ascii_case("$this") {
            relative_class(file, "self", whole.start())
        } else {
            let token = receiver.as_str().trim_end_matches("::class");
            match token.to_ascii_lowercase().as_str() {
                "self" | "static" | "parent" => relative_class(file, token, whole.start()),
                _ => Some(file.resolve(token)),
            }
        };
        if class.is_some_and(|name| family.contains(&name.to_ascii_lowercase())) {
            plan.sites.push(Site::replace(
                &file.rel,
                literal.start(),
                literal.end(),
                new,
            ));
        }
    }
}
