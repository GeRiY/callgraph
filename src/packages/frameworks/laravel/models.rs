//! Eloquent-modell összefoglaló: az osztály ↔ tábla (explicit `$table` vagy a konvenció: `snake_case` többes szám),
//! a tábla oszlopai a migrációkból (`schema.rs`), a `$fillable` / `$guarded` / `$hidden`, a cast-ok (`$casts`
//! tulajdonság vagy `casts()` metódus) és a relációk (`hasMany(X::class)` …) a modell forrásából. Kimenetek: az
//! outline `table:` / `fillable:` / `⚠` sorai az osztály fejléce alatt, a reláció célja az `orm_relation` sor
//! végén, a json `model` mezője és a `models` nézet.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde::ser::SerializeMap;

use super::super::files::ProjectFiles;
use super::super::inertia::render::{literal, split_top};
use super::Laravel;
use super::routes::{matching, short_class, strip_comments};
use super::rules::method_parts;
use super::schema::{self, Column, Schema, snake, table_of_class};
use crate::i18n::Lang;
use crate::model::FileGraph;

/// A migrációk mappája a gyökérhez relatívan.
const MIGRATIONS_DIR: &str = "database/migrations";
/// Egy felsorolásban legfeljebb ennyi név (a többi `, +N`).
const MAX_NAMES: usize = 8;
/// A `table:` sorban legfeljebb ennyi oszlop (a többi `, +N`).
const MAX_COLUMNS: usize = 60;
/// A `models` nézet összegzésében legfeljebb ennyi modell nélküli tábla.
const MAX_TABLES: usize = 8;

/// Az Eloquent-alaposztályok teljes nevei.
const BASES: [&str; 5] = [
    "Illuminate\\Database\\Eloquent\\Model",
    "Illuminate\\Foundation\\Auth\\User",
    "Illuminate\\Database\\Eloquent\\Relations\\Pivot",
    "Illuminate\\Database\\Eloquent\\Relations\\MorphPivot",
    "Illuminate\\Notifications\\DatabaseNotification",
];
/// A fel nem oldott (importálatlan) alaposztály-nevek.
const SHORT_BASES: [&str; 3] = ["Model", "Authenticatable", "Pivot"];
/// A framework saját táblái: nem számítanak „modell nélküli” táblának.
const FRAMEWORK_TABLES: [&str; 10] = [
    "password_reset_tokens",
    "password_resets",
    "sessions",
    "cache",
    "cache_locks",
    "jobs",
    "job_batches",
    "failed_jobs",
    "migrations",
    "personal_access_tokens",
];
/// A to-many relációk.
const TO_MANY: [&str; 6] = [
    "hasMany",
    "belongsToMany",
    "morphMany",
    "morphToMany",
    "morphedByMany",
    "hasManyThrough",
];

/// Egy reláció-metódus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Relation {
    pub method: String,
    pub kind: String,
    /// A cél osztály rövid neve (`morphTo`-nál nincs).
    pub target: Option<String>,
    /// A `belongsToMany` explicit köztes táblája.
    pub pivot: Option<String>,
}

impl Relation {
    fn many(&self) -> bool {
        TO_MANY.contains(&self.kind.as_str())
    }
}

/// Egy Eloquent-modell a forrásából.
#[derive(Debug, Clone)]
pub(crate) struct Model {
    /// A teljes osztálynév.
    pub class: String,
    /// A fájl a gyökérhez relatívan.
    pub file: String,
    pub table: String,
    pub fillable: Vec<String>,
    pub guarded: Vec<String>,
    pub hidden: Vec<String>,
    pub casts: Vec<(String, String)>,
    pub relations: Vec<Relation>,
}

impl Model {
    fn short(&self) -> &str {
        short_class(&self.class)
    }

    /// A `fillable` és a cast-ok közül azok, amelyek nem oszlopok: (fillable, cast). Ha a tábla nem ismert: üres.
    fn unknown(&self, schema: &Schema) -> (Vec<String>, Vec<String>) {
        let Some(columns) = schema.get(&self.table) else {
            return (Vec::new(), Vec::new());
        };
        let missing = |name: &str| {
            name != "*"
                && !name.contains(['.', '>'])
                && !columns.iter().any(|column| column.name == name)
        };
        (
            self.fillable
                .iter()
                .filter(|name| missing(name))
                .cloned()
                .collect(),
            self.casts
                .iter()
                .map(|(name, _)| name)
                .filter(|name| missing(name))
                .cloned()
                .collect(),
        )
    }
}

/// A modell-ellenőrzéshez beolvasott adatok: a modellek és a séma.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModelData {
    pub models: Vec<Model>,
    pub schema: Schema,
}

/// Rendezett párok JSON-objektumként (`{"email_verified_at":"datetime"}`).
#[derive(Debug, Clone)]
struct Pairs(Vec<(String, String)>);

impl Serialize for Pairs {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut map = ser.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// A json `model.columns` eleme.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ColumnJson {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    nullable: bool,
    unique: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    fk: Option<String>,
}

/// A json `model.relations` eleme.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RelationJson {
    method: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
}

/// A json `model` mező.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelJson {
    table: String,
    columns: Vec<ColumnJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fillable: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    guarded: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hidden: Vec<String>,
    #[serde(skip_serializing_if = "Pairs::is_empty")]
    casts: Pairs,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    relations: Vec<RelationJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    issues: Vec<String>,
}

impl Pairs {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Legfeljebb `MAX_NAMES` elem, a többi `, +N`.
fn capped(items: &[String], max: usize) -> String {
    let mut text = items
        .iter()
        .take(max)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > max {
        text.push_str(&format!(", +{}", items.len() - max));
    }
    text
}

/// Egy oszlop tömör alakja: `név[:típus][?][!]` — a sima string és az `id` típusjelzés nélkül, az idegen kulcs
/// `fk→tábla`.
fn column_text(column: &Column) -> String {
    let ty = match (&column.fk, column.ty.as_str()) {
        (Some(table), _) => format!(":fk→{table}"),
        (None, "string" | "id") => String::new(),
        (None, ty) => format!(":{ty}"),
    };
    format!(
        "{}{ty}{}{}",
        column.name,
        if column.nullable { "?" } else { "" },
        if column.unique { "!" } else { "" }
    )
}

/// Az oszlopok tömören: a `created_at` + `updated_at` páros `timestamps`, a `deleted_at` `softDeletes`.
fn columns_text(columns: &[Column]) -> String {
    let has = |name: &str| columns.iter().any(|c| c.name == name && c.ty == "ts");
    let stamps = has("created_at") && has("updated_at");
    let mut parts: Vec<String> = Vec::new();
    for column in columns {
        let part = match column.name.as_str() {
            "created_at" if stamps && column.ty == "ts" => "timestamps".to_owned(),
            "updated_at" if stamps && column.ty == "ts" => continue,
            "deleted_at" if column.ty == "ts" => "softDeletes".to_owned(),
            _ => column_text(column),
        };
        parts.push(part);
    }
    if parts.len() > MAX_COLUMNS {
        let rest = parts.len() - MAX_COLUMNS;
        parts.truncate(MAX_COLUMNS);
        parts.push(format!("+{rest}"));
    }
    parts.join(", ")
}

/// Az osztály Eloquent-alaposztályra végződik-e (a `extends` szerint).
fn is_base(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    if name.contains('\\') {
        BASES.contains(&name) || name.ends_with("\\Eloquent\\Model")
    } else {
        SHORT_BASES.contains(&name)
    }
}

/// A (rövid vagy teljes) extends-név modell-alaposztály-e: a sor szűréséhez a lekérdezett fájlban.
pub(crate) fn extends_model_base(name: &str) -> bool {
    is_base(name)
}

/// A `[..]` / `array(..)` literál belseje a `from` helytől.
fn array_at(text: &str, from: usize) -> Option<&str> {
    let rest = text.get(from..)?;
    let skipped = rest.len() - rest.trim_start().len();
    let start = from + skipped;
    let tail = text.get(start..)?;
    let open = if tail.starts_with('[') {
        start
    } else if tail.to_ascii_lowercase().starts_with("array(") {
        start + "array".len()
    } else {
        return None;
    };
    let close = matching(text, open, text.len())?;
    text.get(open + 1..close)
}

/// A tulajdonság (`protected $fillable = [..]`) tömb-literáljának belseje.
fn property_array<'a>(clean: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!(
        r"(?:protected|public|private)\s+(?:static\s+)?(?:array\s+)?\${}\s*=\s*",
        regex::escape(name)
    );
    let found = Regex::new(&pattern).ok()?.find(clean)?;
    array_at(clean, found.end())
}

/// Az elemek literál-értékei (`'a', 'b'` → `a`, `b`); a nem literál elem kimarad.
fn list_items(inner: &str) -> Vec<String> {
    split_top(inner, ",")
        .into_iter()
        .filter_map(|item| literal(item.trim()))
        .collect()
}

/// A cast-érték szövege: literál (`datetime`), `X::class` (`X`), különben a kifejezés.
fn cast_value(value: &str) -> String {
    let value = value.trim();
    if let Some(text) = literal(value) {
        return text;
    }
    match value.strip_suffix("::class") {
        Some(class) => short_class(class.trim_start_matches('\\')).to_owned(),
        None => value.split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// A `'kulcs' => érték` elemek (kulcs literál; a többi kimarad).
fn cast_items(inner: &str) -> Vec<(String, String)> {
    split_top(inner, ",")
        .into_iter()
        .filter_map(|item| {
            let parts = split_top(item, "=>");
            let [key, value] = parts.as_slice() else {
                return None;
            };
            Some((literal(key.trim())?, cast_value(value)))
        })
        .collect()
}

/// A `$casts` tulajdonság és a `casts()` metódus `return [..]` tömbje.
fn casts_of(clean: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = property_array(clean, "casts")
        .map(cast_items)
        .unwrap_or_default();
    if let Some((_, body)) = method_parts(clean, "casts")
        && let Some(at) = body.find("return")
        && let Some(inner) = array_at(&body, at + "return".len())
    {
        for (key, value) in cast_items(inner) {
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => out.push((key, value)),
            }
        }
    }
    out
}

/// A reláció célja az első argumentumból: `Role::class` / `'App\Models\Role'` → `Role`.
fn target_class(arg: &str) -> Option<String> {
    let arg = arg.trim();
    let class = match arg.strip_suffix("::class") {
        Some(class) => class.trim().to_owned(),
        None => literal(arg)?,
    };
    let short = short_class(class.trim_start_matches('\\'));
    (!short.is_empty()).then(|| short.to_owned())
}

/// A modell reláció-metódusai: a törzsben az első `return $this->hasMany(X::class, ..)`-szerű hívás.
fn relations_of(clean: &str) -> Vec<Relation> {
    static FUNCTION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"function\s+&?\s*(\w+)\s*\(").expect("valid regex"));
    static RELATION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"\$this\s*->\s*(hasManyThrough|hasOneThrough|hasMany|hasOne|belongsToMany|belongsTo|morphToMany|morphedByMany|morphMany|morphOne|morphTo)\s*\(",
        )
        .expect("valid regex")
    });
    let mut out: Vec<Relation> = Vec::new();
    for caps in FUNCTION.captures_iter(clean) {
        let name = &caps[1];
        if name == "casts" || name.starts_with("scope") || name.starts_with("__") {
            continue;
        }
        if out.iter().any(|relation| relation.method == name) {
            continue;
        }
        let Some((_, body)) = method_parts(clean, name) else {
            continue;
        };
        let Some(found) = RELATION.captures(&body) else {
            continue;
        };
        let open = found.get(0).expect("group 0").end() - 1;
        let args: Vec<&str> = matching(&body, open, body.len())
            .and_then(|close| body.get(open + 1..close))
            .map(|inner| split_top(inner, ","))
            .unwrap_or_default();
        let kind = found[1].to_owned();
        let target = if kind == "morphTo" {
            None
        } else {
            args.first().and_then(|arg| target_class(arg))
        };
        let pivot = (kind == "belongsToMany")
            .then(|| args.get(1).and_then(|arg| literal(arg.trim())))
            .flatten();
        out.push(Relation {
            method: name.to_owned(),
            kind,
            target,
            pivot,
        });
    }
    out
}

/// Egy osztály modell-tényei a (megtisztított) osztálytörzsből.
pub(super) fn parse_model(class: &str, file: &str, clean: &str) -> Model {
    static TABLE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:protected|public|private)\s+(?:\?string\s+|string\s+)?\$table\s*=\s*['"]([^'"]+)['"]"#)
            .expect("valid regex")
    });
    let short = short_class(class);
    let table = TABLE
        .captures(clean)
        .map_or_else(|| table_of_class(short), |caps| caps[1].to_owned());
    let list = |name: &str| {
        property_array(clean, name)
            .map(list_items)
            .unwrap_or_default()
    };
    Model {
        class: class.to_owned(),
        file: file.to_owned(),
        table,
        fillable: list("fillable"),
        guarded: list("guarded"),
        hidden: list("hidden"),
        casts: casts_of(clean),
        relations: relations_of(clean),
    }
}

/// Az osztály törzse a fájl szövegéből (a sortartomány), megjegyzések nélkül.
fn class_text(text: &str, line: u32, end_line: u32) -> String {
    let skip = usize::try_from(line).unwrap_or(1).saturating_sub(1);
    let take = usize::try_from(end_line.saturating_sub(line)).unwrap_or(0) + 1;
    let region: Vec<&str> = text.lines().skip(skip).take(take).collect();
    strip_comments(&region.join("\n"))
}

impl Laravel {
    /// A modellek és a séma a projektre: a `class X extends Model` (közvetve is) osztályok forrásukból, a
    /// `database/migrations/*.php` fájlok `up()` metódusaiból a séma.
    pub(crate) fn scan_models<'g>(
        &mut self,
        graphs: impl IntoIterator<Item = (&'g Path, &'g FileGraph)>,
        files: &dyn ProjectFiles,
    ) {
        // osztály → (szülő, fájl-út, rel, sorok)
        struct Class<'g> {
            parent: Option<String>,
            path: &'g Path,
            rel: String,
            line: u32,
            end_line: u32,
        }
        let mut classes: BTreeMap<String, Class<'g>> = BTreeMap::new();
        for (path, graph) in graphs {
            if !graph.file.ends_with(".php") {
                continue;
            }
            let Some(rel) = self.relative_path(path) else {
                continue;
            };
            let namespace = graph.namespace.as_deref().unwrap_or("").trim_matches('\\');
            for container in graph.containers.iter().filter(|c| c.kind == "class") {
                let full = if namespace.is_empty() {
                    container.name.clone()
                } else {
                    format!("{namespace}\\{}", container.name)
                };
                classes.insert(
                    full,
                    Class {
                        parent: container
                            .extends
                            .as_ref()
                            .and_then(|e| e.first())
                            .map(|e| e.trim_start_matches('\\').to_owned()),
                        path,
                        rel: rel.clone(),
                        line: container.line,
                        end_line: container.end_line,
                    },
                );
            }
        }
        let is_model = |full: &str| {
            let mut current = classes.get(full).and_then(|c| c.parent.as_deref());
            for _ in 0..10 {
                let Some(parent) = current else {
                    return false;
                };
                if is_base(parent) {
                    return true;
                }
                current = classes.get(parent).and_then(|c| c.parent.as_deref());
            }
            false
        };
        let mut models: Vec<Model> = Vec::new();
        for (full, class) in &classes {
            if !is_model(full) {
                continue;
            }
            let Some(text) = files.read(class.path) else {
                continue;
            };
            let clean = class_text(&text, class.line, class.end_line);
            models.push(parse_model(full, &class.rel, &clean));
        }
        models.sort_by(|a, b| a.class.cmp(&b.class));
        let migrations: Vec<String> = files
            .files_under(&self.root.join(MIGRATIONS_DIR))
            .iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "php"))
            .filter_map(|path| files.read(path))
            .collect();
        let schema = schema::build(migrations.iter().map(String::as_str));
        self.models = Some(ModelData { models, schema });
    }

    /// A modell az osztály teljes neve (`namespace` + `container`) szerint, és a séma.
    fn model_of(&self, namespace: Option<&str>, container: &str) -> Option<(&Model, &Schema)> {
        let data = self.models.as_ref()?;
        let full = match namespace.filter(|ns| !ns.is_empty()) {
            Some(ns) => format!("{}\\{container}", ns.trim_matches('\\')),
            None => container.to_owned(),
        };
        let model = data.models.iter().find(|model| model.class == full)?;
        Some((model, &data.schema))
    }

    /// Az osztály-fejléc alatti sorok: `table:`, a `fillable: .. · casts: ..`, és a `⚠` figyelmeztetések. Nem
    /// modell (vagy nem futott a modell-keresés): üres.
    pub(crate) fn model_lines(
        &self,
        namespace: Option<&str>,
        container: &str,
        lang: Lang,
    ) -> Vec<String> {
        let Some((model, schema)) = self.model_of(namespace, container) else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        lines.push(match schema.get(&model.table) {
            Some(columns) if !columns.is_empty() => {
                format!("table: {} · {}", model.table, columns_text(columns))
            }
            Some(_) => format!("table: {}", model.table),
            None => format!(
                "table: {} {}",
                model.table,
                lang.pick("(no migration found)", "(nincs migráció)")
            ),
        });
        let mut facts = Vec::new();
        if !model.fillable.is_empty() {
            facts.push(format!("fillable: {}", capped(&model.fillable, MAX_NAMES)));
        }
        if !model.casts.is_empty() {
            let casts: Vec<String> = model
                .casts
                .iter()
                .map(|(name, ty)| format!("{name}:{ty}"))
                .collect();
            facts.push(format!("casts: {}", capped(&casts, MAX_NAMES)));
        }
        if !facts.is_empty() {
            lines.push(facts.join(" · "));
        }
        lines.extend(issue_lines(model, schema, lang));
        lines
    }

    /// A reláció-metódus célja az `orm_relation` sor végére: `Role`; nincs ilyen reláció / cél: `None`.
    pub(crate) fn relation_target(
        &self,
        namespace: Option<&str>,
        container: &str,
        method: &str,
    ) -> Option<String> {
        let (model, _) = self.model_of(namespace, container)?;
        model
            .relations
            .iter()
            .find(|relation| relation.method == method)?
            .target
            .clone()
    }

    /// A json `model` mező az osztályhoz; nem modell: `None`.
    pub(crate) fn model_json(
        &self,
        namespace: Option<&str>,
        container: &str,
    ) -> Option<serde_json::Value> {
        let (model, schema) = self.model_of(namespace, container)?;
        let columns = schema.get(&model.table);
        let mut issues = issue_lines_en(model, schema);
        if columns.is_none() {
            issues.insert(0, "no migration found".to_owned());
        }
        let json = ModelJson {
            table: model.table.clone(),
            columns: columns
                .map(|columns| {
                    columns
                        .iter()
                        .map(|c| ColumnJson {
                            name: c.name.clone(),
                            ty: c.ty.clone(),
                            nullable: c.nullable,
                            unique: c.unique,
                            fk: c.fk.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            fillable: model.fillable.clone(),
            guarded: model.guarded.clone(),
            hidden: model.hidden.clone(),
            casts: Pairs(model.casts.clone()),
            relations: model
                .relations
                .iter()
                .map(|relation| RelationJson {
                    method: relation.method.clone(),
                    kind: relation.kind.clone(),
                    target: relation.target.clone(),
                })
                .collect(),
            issues,
        };
        serde_json::to_value(&json).ok()
    }

    /// A `models` nézet sorai a hatókörre (`scope`: a lekérdezett fájlok a gyökérhez relatívan): modellenként egy
    /// sor, utána a problémák (`⚠`) és az összegzés. Üres: nincs modell a hatókörben.
    pub(crate) fn model_view_lines(&self, scope: &[String], lang: Lang) -> Vec<String> {
        let Some(data) = self.models.as_ref() else {
            return Vec::new();
        };
        let mut shown: Vec<&Model> = data
            .models
            .iter()
            .filter(|model| scope.contains(&model.file))
            .collect();
        if shown.is_empty() {
            return Vec::new();
        }
        shown.sort_by(|a, b| a.short().cmp(b.short()).then_with(|| a.class.cmp(&b.class)));
        let mut lines: Vec<String> = Vec::new();
        for model in &shown {
            let count = data
                .schema
                .get(&model.table)
                .map_or_else(|| "?".to_owned(), |columns| columns.len().to_string());
            let mut line = format!("{} {}({count})", model.short(), model.table);
            if !model.relations.is_empty() {
                let relations: Vec<String> = model
                    .relations
                    .iter()
                    .map(|relation| {
                        format!(
                            "{}:{}{}",
                            relation.method,
                            relation.target.as_deref().unwrap_or("?"),
                            if relation.many() { "*" } else { "" }
                        )
                    })
                    .collect();
                line.push_str(&format!(" → {}", relations.join(", ")));
            }
            lines.push(line);
        }
        for model in &shown {
            for issue in issue_lines(model, &data.schema, lang) {
                lines.push(format!(
                    "⚠ {}: {}",
                    model.short(),
                    issue.trim_start_matches("⚠ ")
                ));
            }
        }
        let orphans = tables_without_model(data);
        let tail = if orphans.is_empty() {
            String::new()
        } else {
            let more = orphans.len().saturating_sub(MAX_TABLES);
            let mut names = orphans
                .iter()
                .take(MAX_TABLES)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if more > 0 {
                names.push_str(&format!(", +{more}"));
            }
            match lang {
                Lang::En => format!(", tables without model: {names}"),
                Lang::Hu => format!(", modell nélküli táblák: {names}"),
            }
        };
        lines.push(match lang {
            Lang::En => format!("models: {}{tail}", shown.len()),
            Lang::Hu => format!("modellek: {}{tail}", shown.len()),
        });
        lines
    }

    /// A modellek (teszthez).
    #[cfg(test)]
    pub(crate) fn models(&self) -> &[Model] {
        self.models
            .as_ref()
            .map_or(&[], |data| data.models.as_slice())
    }
}

/// A `⚠` sorok a modellhez (az `unknown` oszlopok): `⚠ fillable not a column: nick, foo`.
fn issue_lines(model: &Model, schema: &Schema, lang: Lang) -> Vec<String> {
    let (fillable, casts) = model.unknown(schema);
    let mut lines = Vec::new();
    if !fillable.is_empty() {
        lines.push(format!(
            "⚠ {} {}",
            lang.pick("fillable not a column:", "fillable, de nincs ilyen oszlop:"),
            capped(&fillable, MAX_NAMES)
        ));
    }
    if !casts.is_empty() {
        lines.push(format!(
            "⚠ {} {}",
            lang.pick("cast not a column:", "cast, de nincs ilyen oszlop:"),
            capped(&casts, MAX_NAMES)
        ));
    }
    lines
}

/// A json problémái (angolul, `⚠` nélkül).
fn issue_lines_en(model: &Model, schema: &Schema) -> Vec<String> {
    issue_lines(model, schema, Lang::En)
        .into_iter()
        .map(|line| line.trim_start_matches("⚠ ").to_owned())
        .collect()
}

/// A modell nélküli táblák (a framework saját táblái és a köztes táblák nélkül), ábécében.
fn tables_without_model(data: &ModelData) -> Vec<String> {
    let mut used: BTreeSet<String> = data.models.iter().map(|m| m.table.clone()).collect();
    for model in &data.models {
        for relation in &model.relations {
            if let Some(pivot) = &relation.pivot {
                used.insert(pivot.clone());
            } else if relation.kind == "belongsToMany"
                && let Some(target) = &relation.target
            {
                let mut pair = [snake(model.short()), snake(target)];
                pair.sort();
                used.insert(pair.join("_"));
            }
        }
    }
    data.schema
        .keys()
        .filter(|table| !used.contains(*table) && !FRAMEWORK_TABLES.contains(&table.as_str()))
        .cloned()
        .collect()
}
