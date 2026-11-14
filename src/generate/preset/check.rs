//! A preset ellenőrzései: az önellenőrzés (a generált fájlokon az űrlap-szerződés és a route-nevek, a Laravel-modul
//! F1 / F2 logikájával) és a modell-figyelmeztetések (a mezők oszlopok-e, benne vannak-e a `$fillable`-ben).

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use super::plan::Plan;
use super::spec::{Field, Names};
use crate::generate::fs::Env;
use crate::packages::frameworks::laravel::preset as laravel;

/// Az önellenőrzés sorai: `✓ form ↔ rules (4 fields) · route names (3)`, vagy a talált hibák.
pub fn self_check(plan: &Plan) -> Vec<String> {
    static ROUTE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b(?:to_)?route\(\s*'([^']+)'").expect("valid regex"));
    let mut issues: Vec<String> = Vec::new();
    let mut good: Vec<String> = Vec::new();

    let (mut forms, mut fields) = (0usize, 0usize);
    for (page, request) in &plan.forms {
        let Some(contract) = laravel::form_contract(page, request) else {
            continue;
        };
        forms += 1;
        fields += contract.fields.len();
        if !contract.unvalidated.is_empty() {
            issues.push(crate::tf!(
                "✗ fields sent without a rule: {}",
                "✗ szabály nélkül küldött mezők: {}",
                contract.unvalidated.join(", ")
            ));
        }
        if !contract.missing.is_empty() {
            issues.push(crate::tf!(
                "✗ required by the rules but not sent: {}",
                "✗ a szabály kötelezi, de nem küldött: {}",
                contract.missing.join(", ")
            ));
        }
    }
    if forms > 0 && issues.is_empty() {
        good.push(if forms == 1 {
            crate::tf!("form ↔ rules ({} fields)", "form ↔ rules ({} mező)", fields)
        } else {
            crate::tf!(
                "form ↔ rules ({} forms, {} fields)",
                "form ↔ rules ({} form, {} mező)",
                forms,
                fields
            )
        });
    }

    let mut used: BTreeSet<String> = BTreeSet::new();
    for file in &plan.files {
        let text = file.shown.join("\n");
        for found in ROUTE.captures_iter(&text) {
            used.insert(found[1].to_owned());
        }
    }
    let unknown: Vec<&str> = used
        .iter()
        .filter(|name| !plan.verified_routes.contains(*name))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        issues.push(crate::tf!(
            "✗ unknown route name(s): {}",
            "✗ ismeretlen route-név: {}",
            unknown.join(", ")
        ));
    } else if !used.is_empty() {
        good.push(crate::tf!(
            "route names ({})",
            "route-nevek ({})",
            used.len()
        ));
    }

    let mut out = Vec::new();
    if !good.is_empty() {
        out.push(format!("✓ {}", good.join(" · ")));
    }
    out.extend(issues);
    out
}

/// A modell-figyelmeztetések (nem hiba): hiányzó modell, a mező nem oszlop, nincs a `$fillable`-ben.
pub fn model_notes(env: Env<'_>, plan: &Plan, names: &Names, fields: &[Field]) -> Vec<String> {
    let model = &names.model;
    let Some(rel) = &plan.model_file else {
        return vec![crate::tf!(
            "⚠ model {} not found (app/Models/{}.php) — no model or migration generated",
            "⚠ a(z) {} modell nincs meg (app/Models/{}.php) — modellt, migrációt nem generáltam",
            model,
            model
        )];
    };
    let Ok(text) = env.fs.read(&env.root.join(rel)) else {
        return Vec::new();
    };
    let facts = laravel::model_facts(&plan.model_class, rel, &text, &migrations(env));
    let mut notes = Vec::new();
    match &facts.columns {
        Some(columns) => {
            let missing: Vec<&str> = fields
                .iter()
                .map(|f| f.name.as_str())
                .filter(|name| !columns.iter().any(|c| c == *name))
                .collect();
            if !missing.is_empty() {
                notes.push(crate::tf!(
                    "⚠ {}: not a column of `{}`: {} (migration missing?)",
                    "⚠ {}: nem oszlopa a(z) `{}` táblának: {} (hiányzik a migráció?)",
                    model,
                    facts.table,
                    missing.join(", ")
                ));
            }
            // NOT NULL, alapérték nélküli oszlop, amit az űrlap nem küld: a `create()` futáskor elbukna
            let unsent: Vec<&str> = facts
                .required
                .iter()
                .map(String::as_str)
                .filter(|column| !fields.iter().any(|f| f.name == *column))
                .collect();
            if !unsent.is_empty() {
                notes.push(crate::tf!(
                    "⚠ {}: required column not sent: {} (NOT NULL, no default — create() would fail; set it in the controller or add it to the form)",
                    "⚠ {}: kötelező oszlop, amit nem küld: {} (NOT NULL, nincs alapérték — a create() elbukna; állítsd be a controllerben, vagy vedd fel az űrlapra)",
                    model,
                    unsent.join(", ")
                ));
            }
        }
        None => notes.push(crate::tf!(
            "⚠ table `{}` is not created by any migration",
            "⚠ a(z) `{}` táblát egy migráció sem hozza létre",
            facts.table
        )),
    }
    if !facts.guarded {
        if facts.fillable.is_empty() {
            notes.push(crate::tf!(
                "⚠ {}: no $fillable — mass assignment of validated() would fail",
                "⚠ {}: nincs $fillable — a validated() tömeges értékadása hibázna",
                model
            ));
        } else {
            let missing: Vec<&str> = fields
                .iter()
                .map(|f| f.name.as_str())
                .filter(|name| !facts.fillable.iter().any(|f| f == *name))
                .collect();
            if !missing.is_empty() {
                notes.push(crate::tf!(
                    "⚠ {}: not in $fillable: {} (silently dropped on create / update)",
                    "⚠ {}: nincs a $fillable-ben: {} (create / update során csendben kimarad)",
                    model,
                    missing.join(", ")
                ));
            }
        }
    }
    notes
}

/// A migrációk szövege fájlnév-sorrendben.
fn migrations(env: Env<'_>) -> Vec<String> {
    let mut files: Vec<_> = env
        .fs
        .list(&env.root.join("database/migrations"))
        .unwrap_or_default()
        .into_iter()
        .filter(|e| !e.is_dir && e.path.extension().is_some_and(|x| x == "php"))
        .map(|e| e.path)
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|path| env.fs.read(path).ok())
        .collect()
}
