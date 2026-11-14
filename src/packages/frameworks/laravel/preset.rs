//! A kódgenerátor Laravel-presetjének (`crate::generate::preset`) vékony, kifelé adott felülete: a route-tábla, az
//! űrlap-szerződés és a modell-összefoglaló tényei egy-egy forrásszövegből (nincs gráf, nincs `ProjectFiles`), hogy a
//! preset az éppen generált (még nem lemezen lévő) szövegeken is ellenőrizhessen. Ami nem értelmezhető, kimarad.

use std::sync::LazyLock;

use regex::Regex;

use super::forms::{compare, object_keys};
use super::models::parse_model;
use super::routes::{matching, parse_file, strip_comments};
use super::rules::request_validation;
use super::schema::{build, table_of_class};
use crate::model::Imports;

/// Egy route a táblából (a preset ütközés-vizsgálatához).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteFacts {
    pub name: Option<String>,
    pub verb: String,
    pub uri: String,
}

/// Egy route-fájl route-jai a forrásából.
pub(crate) fn routes_of(rel: &str, text: &str) -> Vec<RouteFacts> {
    parse_file(rel, text, &Imports::default())
        .into_iter()
        .map(|route| RouteFacts {
            name: route.name,
            verb: route.verb,
            uri: route.uri,
        })
        .collect()
}

/// A modell konvenció szerinti táblaneve (`ServiceJob` → `service_jobs`).
pub(crate) fn table_name(model: &str) -> String {
    table_of_class(model)
}

/// Az `auth` middleware-es `Route::middleware(..)->group(function () { .. })` záró `}` sorának 0-alapú indexe a
/// szövegben (az első ilyen csoport; láncolt `prefix` / `name` nélküli csoport, hogy az URI-k ne csússzanak el).
pub(crate) fn auth_group_close_line(text: &str) -> Option<usize> {
    static GROUP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"Route::middleware\(([^)]*)\)\s*->\s*group\(\s*(?:static\s+)?function\s*\(\s*\)\s*\{",
        )
        .expect("valid regex")
    });
    static TOKEN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"['"]([^'"]+)['"]"#).expect("valid regex"));
    let clean = strip_comments(text);
    for found in GROUP.captures_iter(&clean) {
        let args = found.get(1).map_or("", |m| m.as_str());
        let is_auth = TOKEN
            .captures_iter(args)
            .any(|t| &t[1] == "auth" || t[1].starts_with("auth:"));
        if !is_auth {
            continue;
        }
        let open = found.get(0)?.end() - 1;
        let close = matching(&clean, open, clean.len())?;
        return Some(clean.get(..close)?.matches('\n').count());
    }
    None
}

/// Az űrlap-szerződés egy oldal és a kérés-osztály között.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Contract {
    pub fields: Vec<String>,
    /// A küldött, de szabály nélküli mezők.
    pub unvalidated: Vec<String>,
    /// A kötelező szabályú, de nem küldött kulcsok.
    pub missing: Vec<String>,
}

/// Az oldal `useForm({..})` mezői a FormRequest `rules()` szabályaival összevetve; `None`, ha nincs literál
/// `useForm` vagy nincs szabály-forrás.
pub(crate) fn form_contract(page: &str, request: &str) -> Option<Contract> {
    let at = page.find("useForm(")?;
    let open = at + "useForm".len();
    let close = matching(page, open, page.len())?;
    let fields = object_keys(page.get(open + 1..close)?)?;
    let validation = request_validation(request);
    if !validation.found {
        return None;
    }
    let (unvalidated, missing) = compare(&fields, &validation);
    Some(Contract {
        fields,
        unvalidated,
        missing,
    })
}

/// A modell tényei a forrásából és a migrációkból.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelFacts {
    pub table: String,
    pub fillable: Vec<String>,
    /// A modell említi a `$guarded`-et (akkor a `$fillable` hiánya nem hiba).
    pub guarded: bool,
    /// A tábla oszlopai a migrációkból; `None`, ha a tábla nincs a migrációkban.
    pub columns: Option<Vec<String>>,
}

/// A modell tényei: `migrations`: a migrációk szövege fájlnév-sorrendben.
pub(crate) fn model_facts(
    class: &str,
    file: &str,
    text: &str,
    migrations: &[String],
) -> ModelFacts {
    let clean = strip_comments(text);
    let model = parse_model(class, file, &clean);
    let schema = build(migrations.iter().map(String::as_str));
    ModelFacts {
        columns: schema
            .get(&model.table)
            .map(|columns| columns.iter().map(|c| c.name.clone()).collect()),
        guarded: clean.contains("$guarded"),
        table: model.table,
        fillable: model.fillable,
    }
}
