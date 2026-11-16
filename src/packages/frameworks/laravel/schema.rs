//! Adatbázis-séma a migrációkból (`database/migrations/*.php`, fájlnév-sorrendben): a `Schema::create('t', ..)` /
//! `Schema::table('t', ..)` closure-ök `$table->típus('név')->módosítók()` láncai a `up()` metódusban (a `down()` nem
//! számít), a `dropColumn` / `renameColumn` / `->change()` módosítások, és a `Schema::dropIfExists` / `drop` /
//! `rename`. Csak a forrásból, regexszel és zárójel-párosítással (nincs PHP-interpreter): ami nem értelmezhető,
//! csendben kimarad.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

use super::super::inertia::render::{literal, split_top};
use super::routes::{matching, strip_comments};
use super::rules::method_parts;

/// Egy tábla egy oszlopa.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Column {
    pub name: String,
    /// Rövid típusnév: `id`, `string`, `text`, `int`, `bigint`, `bool`, `dec`, `float`, `date`, `dt`, `ts`, `time`,
    /// `json`, `uuid`, `ulid`, `enum`, `char`, `bin`, `fk`.
    pub ty: String,
    pub nullable: bool,
    pub unique: bool,
    /// Idegen kulcs célja (tábla).
    pub fk: Option<String>,
    /// Van-e alapértéke (`default(..)`, `useCurrent()`, generált oszlop): beszúráskor elhagyható.
    pub default: bool,
}

impl Column {
    /// Beszúráskor kötelező-e: nem nullable, nincs alapértéke, és nem automatikus kulcs (`id`, `increments`).
    pub(crate) fn required_on_insert(&self) -> bool {
        !self.nullable && !self.default && self.ty != "id"
    }
}

/// Tábla neve → oszlopai (a létrehozás / hozzáadás sorrendjében).
pub(crate) type Schema = BTreeMap<String, Vec<Column>>;

/// A migrációk szövege (fájlnév-sorrendben) a sémára alkalmazva.
pub(super) fn build<'a>(migrations: impl IntoIterator<Item = &'a str>) -> Schema {
    let mut schema = Schema::new();
    for text in migrations {
        apply(&mut schema, text);
    }
    schema
}

/// Egy `PascalCase` osztálynév `snake_case` alakja (a Laravel `Str::snake`-je: minden nagybetű elé `_`).
pub(super) fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Többes szám (kicsi szabályrendszer): szabálytalanok, megszámlálhatatlanok, `y` → `ies` mássalhangzó után,
/// `s` / `x` / `z` / `ch` / `sh` → `es`, különben `s`.
pub(super) fn plural(word: &str) -> String {
    const UNCOUNTABLE: [&str; 9] = [
        "equipment",
        "information",
        "news",
        "series",
        "species",
        "sheep",
        "fish",
        "data",
        "media",
    ];
    let last = word.rsplit('_').next().unwrap_or(word);
    if UNCOUNTABLE.contains(&last) {
        return word.to_owned();
    }
    if let Some(stem) = word.strip_suffix("person") {
        return format!("{stem}people");
    }
    if word.ends_with("child") {
        return format!("{word}ren");
    }
    if let Some(stem) = word.strip_suffix('y') {
        let vowel = stem.ends_with(['a', 'e', 'i', 'o', 'u']);
        return if vowel || stem.is_empty() {
            format!("{word}s")
        } else {
            format!("{stem}ies")
        };
    }
    if word.ends_with(['s', 'x', 'z']) || word.ends_with("ch") || word.ends_with("sh") {
        return format!("{word}es");
    }
    format!("{word}s")
}

/// Az osztály (rövid név) konvenció szerinti táblaneve: `ServiceJob` → `service_jobs`.
pub(super) fn table_of_class(short: &str) -> String {
    plural(&snake(short))
}

/// A `Schema::` hívások (a nyitó zárójelig): létrehozás, módosítás, törlés, átnevezés.
static CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Schema::(?:connection\([^)]*\)->)?(create|table|dropIfExists|drop|rename)\s*\(")
        .expect("valid regex")
});

/// A migráció `up()`-jának tábla-eseményei sorrendben: `(true, t)` létrehozás, `(false, t)` törlés (a
/// `rename` a régi név törlése és az új létrehozása). A kétszer létrehozott tábla felismeréséhez.
pub(super) fn table_events(text: &str) -> Vec<(bool, String)> {
    let clean = strip_comments(text);
    let Some((_, body)) = method_parts(&clean, "up") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(caps) = CALL.captures_at(&body, at) {
        let whole = caps.get(0).expect("group 0");
        let open = whole.end() - 1;
        let Some(close) = matching(&body, open, body.len()) else {
            break;
        };
        at = close + 1;
        let args = split_top(&body[open + 1..close], ",");
        let names: Vec<String> = args.iter().filter_map(|arg| literal(arg)).collect();
        match (&caps[1], names.as_slice()) {
            ("create", [table, ..]) => out.push((true, table.clone())),
            ("dropIfExists" | "drop", tables) => {
                out.extend(tables.iter().map(|t| (false, t.clone())));
            }
            ("rename", [from, to, ..]) => {
                out.push((false, from.clone()));
                out.push((true, to.clone()));
            }
            _ => {}
        }
    }
    out
}

/// Egy migráció `up()` metódusa a sémára alkalmazva.
pub(super) fn apply(schema: &mut Schema, text: &str) {
    let clean = strip_comments(text);
    let Some((_, body)) = method_parts(&clean, "up") else {
        return;
    };
    let mut at = 0;
    while let Some(caps) = CALL.captures_at(&body, at) {
        let whole = caps.get(0).expect("group 0");
        let open = whole.end() - 1;
        let Some(close) = matching(&body, open, body.len()) else {
            break;
        };
        at = close + 1;
        let inner = &body[open + 1..close];
        let args = split_top(inner, ",");
        let table = args.first().and_then(|arg| literal(arg));
        match &caps[1] {
            "create" | "table" => {
                let Some(table) = table else {
                    continue;
                };
                let creating = &caps[1] == "create";
                if creating {
                    schema.insert(table.clone(), Vec::new());
                }
                let Some(columns) = schema.get_mut(&table) else {
                    continue;
                };
                if let Some(closure) = args.get(1) {
                    closure_statements(closure, columns);
                }
            }
            "dropIfExists" | "drop" => {
                for arg in &args {
                    if let Some(name) = literal(arg) {
                        schema.remove(&name);
                    }
                }
            }
            _ => {
                if let (Some(from), Some(to)) = (table, args.get(1).and_then(|arg| literal(arg)))
                    && let Some(columns) = schema.remove(&from)
                {
                    schema.insert(to, columns);
                }
            }
        }
    }
}

/// A closure törzsének `$table->..;` utasításai az oszlopokra alkalmazva.
fn closure_statements(closure: &str, columns: &mut Vec<Column>) {
    let Some(brace) = closure.find('{') else {
        return;
    };
    let Some(end) = matching(closure, brace, closure.len()) else {
        return;
    };
    let head = &closure[..brace];
    let var = head.find('$').map_or("$table".to_owned(), |at| {
        let ident: String = head[at + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        format!("${ident}")
    });
    for statement in split_top(&closure[brace + 1..end], ";") {
        let Some(rest) = statement
            .trim()
            .strip_prefix(var.as_str())
            .and_then(|rest| rest.trim_start().strip_prefix("->"))
        else {
            continue;
        };
        let Some(steps) = split_top(rest, "->")
            .into_iter()
            .map(parse_step)
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        run(columns, &steps);
    }
}

/// Egy lánc-tag: `name(args)`.
struct Step {
    name: String,
    args: Vec<String>,
}

fn parse_step(part: &str) -> Option<Step> {
    let part = part.trim();
    let open = part.find('(')?;
    let name = part[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let close = matching(part, open, part.len())?;
    let inner = part[open + 1..close].trim();
    let args = if inner.is_empty() {
        Vec::new()
    } else {
        split_top(inner, ",")
            .into_iter()
            .map(|arg| arg.trim().to_owned())
            .collect()
    };
    Some(Step {
        name: name.to_owned(),
        args,
    })
}

fn arg_literal(args: &[String], index: usize) -> Option<String> {
    args.get(index).and_then(|arg| literal(arg))
}

/// Literál, nevesített argumentummal is (`table: 'users'`).
fn named_literal(arg: &str) -> Option<String> {
    let text = arg.trim();
    let text = if text.starts_with(['\'', '"']) {
        text
    } else {
        text.split_once(':').map_or(text, |(_, value)| value.trim())
    };
    literal(text)
}

/// A literál nevek: `'a'`, `'a', 'b'` és `['a', 'b']`.
fn names_in(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        let text = arg.trim();
        if let Some(inner) = text
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            out.extend(split_top(inner, ",").into_iter().filter_map(literal));
        } else if let Some(name) = literal(text) {
            out.push(name);
        }
    }
    out
}

fn column(name: &str, ty: &str) -> Column {
    Column {
        name: name.to_owned(),
        ty: ty.to_owned(),
        nullable: false,
        unique: false,
        fk: None,
        default: false,
    }
}

/// A `foreignId('user_id')->constrained()` célja: az oszlopnévből (`user_id` → `users`).
fn guess_table(name: &str) -> String {
    plural(name.strip_suffix("_id").unwrap_or(name))
}

/// A séma-építő metódus rövid típusa.
fn short_type(method: &str) -> Option<&'static str> {
    Some(match method {
        "string" | "ipAddress" | "macAddress" => "string",
        "char" => "char",
        "text" | "tinyText" | "mediumText" | "longText" => "text",
        "integer"
        | "unsignedInteger"
        | "mediumInteger"
        | "unsignedMediumInteger"
        | "smallInteger"
        | "unsignedSmallInteger"
        | "tinyInteger"
        | "unsignedTinyInteger"
        | "year" => "int",
        "bigInteger" | "unsignedBigInteger" => "bigint",
        "boolean" => "bool",
        "decimal" | "unsignedDecimal" => "dec",
        "float" | "double" => "float",
        "date" => "date",
        "dateTime" | "dateTimeTz" => "dt",
        "timestamp" | "timestampTz" => "ts",
        "time" | "timeTz" => "time",
        "json" | "jsonb" => "json",
        "uuid" => "uuid",
        "ulid" => "ulid",
        "enum" | "set" => "enum",
        "binary" => "bin",
        _ => return None,
    })
}

/// Egy utasítás (lánc) alkalmazása az oszlopokra.
fn run(columns: &mut Vec<Column>, steps: &[Step]) {
    let Some((base, mods)) = steps.split_first() else {
        return;
    };
    let first = arg_literal(&base.args, 0);
    let mut defs: Vec<Column> = Vec::new();
    match base.name.as_str() {
        "dropColumn" | "dropColumns" => {
            let names = names_in(&base.args);
            columns.retain(|c| !names.contains(&c.name));
            return;
        }
        "dropTimestamps" | "dropTimestampsTz" => {
            columns.retain(|c| c.name != "created_at" && c.name != "updated_at");
            return;
        }
        "dropSoftDeletes" | "dropSoftDeletesTz" => {
            columns.retain(|c| c.name != "deleted_at");
            return;
        }
        "dropRememberToken" => {
            columns.retain(|c| c.name != "remember_token");
            return;
        }
        "dropMorphs" | "dropNullableMorphs" => {
            if let Some(prefix) = first {
                let (kind, id) = (format!("{prefix}_type"), format!("{prefix}_id"));
                columns.retain(|c| c.name != kind && c.name != id);
            }
            return;
        }
        "renameColumn" => {
            if let (Some(from), Some(to)) = (first, arg_literal(&base.args, 1))
                && let Some(found) = columns.iter_mut().find(|c| c.name == from)
            {
                found.name = to;
            }
            return;
        }
        "unique" => {
            let names = names_in(&base.args);
            if let [one] = names.as_slice()
                && let Some(found) = columns.iter_mut().find(|c| c.name == *one)
            {
                found.unique = true;
            }
            return;
        }
        "foreign" => {
            let names = names_in(&base.args);
            let [name] = names.as_slice() else {
                return;
            };
            let target = mods.iter().find_map(|step| match step.name.as_str() {
                "on" => step.args.first().and_then(|arg| named_literal(arg)),
                "constrained" => Some(
                    step.args
                        .first()
                        .and_then(|arg| named_literal(arg))
                        .unwrap_or_else(|| guess_table(name)),
                ),
                _ => None,
            });
            if let Some(found) = columns.iter_mut().find(|c| c.name == *name) {
                found.fk = target.or_else(|| found.fk.take());
            }
            return;
        }
        "id" => defs.push(column(first.as_deref().unwrap_or("id"), "id")),
        "bigIncrements" | "increments" | "mediumIncrements" | "smallIncrements"
        | "tinyIncrements" => defs.extend(first.map(|name| column(&name, "id"))),
        "foreignId" | "foreignUuid" | "foreignUlid" => {
            defs.extend(first.map(|name| column(&name, "fk")));
        }
        "foreignIdFor" => {
            let class = base
                .args
                .first()
                .and_then(|arg| arg.trim().strip_suffix("::class"))
                .map(|class| class.rsplit('\\').next().unwrap_or(class).to_owned());
            if let Some(class) = class {
                let mut def = column(&format!("{}_id", snake(&class)), "fk");
                def.fk = Some(table_of_class(&class));
                defs.push(def);
            }
        }
        "morphs" | "nullableMorphs" | "uuidMorphs" | "ulidMorphs" | "nullableUuidMorphs"
        | "nullableUlidMorphs" => {
            if let Some(prefix) = first {
                let id_type = if base.name.contains("uuid") || base.name.contains("Uuid") {
                    "uuid"
                } else if base.name.contains("ulid") || base.name.contains("Ulid") {
                    "ulid"
                } else {
                    "bigint"
                };
                defs.push(column(&format!("{prefix}_type"), "string"));
                defs.push(column(&format!("{prefix}_id"), id_type));
                if base.name.starts_with("nullable") {
                    for def in &mut defs {
                        def.nullable = true;
                    }
                }
            }
        }
        "timestamps" | "timestampsTz" | "nullableTimestamps" => {
            for name in ["created_at", "updated_at"] {
                let mut def = column(name, "ts");
                def.nullable = true;
                defs.push(def);
            }
        }
        "softDeletes" | "softDeletesTz" => {
            let mut def = column(first.as_deref().unwrap_or("deleted_at"), "ts");
            def.nullable = true;
            defs.push(def);
        }
        "rememberToken" => {
            let mut def = column("remember_token", "string");
            def.nullable = true;
            defs.push(def);
        }
        other => {
            if let (Some(ty), Some(name)) = (short_type(other), first) {
                defs.push(column(&name, ty));
            }
        }
    }
    let mut change = false;
    for step in mods {
        match step.name.as_str() {
            "nullable" => {
                let value = step.args.first().is_none_or(|arg| arg.trim() != "false");
                for def in &mut defs {
                    def.nullable = value;
                }
            }
            "unique" => {
                for def in &mut defs {
                    def.unique = true;
                }
            }
            "default" | "useCurrent" | "storedAs" | "virtualAs" | "autoIncrement" => {
                for def in &mut defs {
                    def.default = true;
                }
            }
            "constrained" => {
                let given = step.args.first().and_then(|arg| named_literal(arg));
                for def in defs.iter_mut().filter(|d| d.ty == "fk") {
                    let target = given
                        .clone()
                        .or_else(|| def.fk.take())
                        .unwrap_or_else(|| guess_table(&def.name));
                    def.fk = Some(target);
                }
            }
            "on" => {
                if let Some(target) = step.args.first().and_then(|arg| named_literal(arg)) {
                    for def in defs.iter_mut().filter(|d| d.ty == "fk") {
                        def.fk = Some(target.clone());
                    }
                }
            }
            "change" => change = true,
            _ => {}
        }
    }
    for def in defs {
        put(columns, def, change);
    }
}

/// Az oszlop beillesztése: új név hozzáadva, meglévő felülírva (`->change()`-nél az egyediség és az idegen kulcs
/// megmarad, ha az új definíció nem mondja).
fn put(columns: &mut Vec<Column>, mut new: Column, change: bool) {
    match columns.iter().position(|c| c.name == new.name) {
        Some(index) => {
            if change {
                new.unique |= columns[index].unique;
                if new.fk.is_none() {
                    new.fk.clone_from(&columns[index].fk);
                }
            }
            columns[index] = new;
        }
        None => columns.push(new),
    }
}
