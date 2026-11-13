//! `json` (a callgraph `jq -s '.'` paritása) és `jsonl` (soronként egy FileGraph) formázók.

use crate::query::QueryResult;

use super::Formatter;

pub(super) struct OrderedGraph<'a> {
    file: &'a str,
    module: &'a Option<String>,
    framework: &'a Option<String>,
    namespace: &'a Option<String>,
    imports: &'a crate::model::Imports,
    /// A fájl hívásaiban hivatkozott csomagok verziója egyszer (név → verzió); a hívásokon csak a név áll, hogy
    /// a verzió ne ismétlődjön hívásonként. Üres: nem kerül a kimenetbe.
    packages: std::collections::BTreeMap<String, String>,
    containers: &'a [crate::model::Container],
    properties: &'a [crate::model::Property],
    constants: &'a [crate::model::Constant],
    methods: &'a [crate::model::Method],
    calls: Calls<'a>,
    control_flow: &'a [crate::model::ControlFlow],
    strings: &'a [crate::model::StringLit],
    comments: Comments<'a>,
    writes: &'a [crate::model::Write],
    sfc: &'a Option<serde_json::Value>,
    parse_errors: &'a [crate::model::ParseError],
    /// `protect` minta találja (csak ilyenkor kerül a kimenetbe).
    protected: bool,
}

impl serde::Serialize for OrderedGraph<'_> {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let js = matches!(self.comments, Comments::Js(_));
        let mut m = ser.serialize_map(None)?;
        m.serialize_entry("file", self.file)?;
        if let Some(v) = self.module {
            m.serialize_entry("module", v)?;
        }
        m.serialize_entry("framework", self.framework)?;
        m.serialize_entry("namespace", self.namespace)?;
        m.serialize_entry("imports", self.imports)?;
        if !self.packages.is_empty() {
            m.serialize_entry("packages", &self.packages)?;
        }
        m.serialize_entry("containers", self.containers)?;
        m.serialize_entry("properties", self.properties)?;
        m.serialize_entry("constants", self.constants)?;
        m.serialize_entry("methods", self.methods)?;
        m.serialize_entry("calls", &self.calls)?;
        if js {
            m.serialize_entry("comments", &self.comments)?;
        }
        m.serialize_entry("controlFlow", self.control_flow)?;
        m.serialize_entry("strings", self.strings)?;
        if !js {
            m.serialize_entry("comments", &self.comments)?;
        }
        m.serialize_entry("writes", self.writes)?;
        if let Some(v) = self.sfc {
            m.serialize_entry("sfc", v)?;
        }
        // csak szintaktikai hibánál (hiba nélkül a kimenet bájtra változatlan)
        if !self.parse_errors.is_empty() {
            m.serialize_entry("parseErrors", self.parse_errors)?;
        }
        if self.protected {
            m.serialize_entry("protected", &true)?;
        }
        m.end()
    }
}

/// A hívások mezősorrendje nyelvfüggő: a JS-motor `category`-t a `resolved` elé teszi. Csomagtérképpel (a
/// projektnek van manifestje) a hívás végén a csomag: `package` (teljes név; a verzió a fájl `packages` táblájában);
/// Inertia-kötésekkel a render-hívásnál a page-fájl: `inertia_page` (és ha nem létezik, `inertia_page_missing`).
#[derive(serde::Serialize)]
#[serde(untagged)]
enum Calls<'a> {
    Php(&'a [crate::model::Call]),
    PhpPackages(Vec<PhpCall<'a>>),
    Js(Vec<JsCall<'a>>),
}

/// PHP- (nem JS-) sorrend: a `Call` saját mezői, utána a csomag, ha a cél egy csomagé.
#[derive(serde::Serialize)]
struct PhpCall<'a> {
    #[serde(flatten)]
    call: &'a crate::model::Call,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<String>,
    /// Inertia render-hívásnál a page-fájl a gyökérhez relatívan (`resources/js/Pages/Board.vue`).
    #[serde(skip_serializing_if = "Option::is_none")]
    inertia_page: Option<String>,
    /// Csak ha a page-fájl nem létezik: `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    inertia_page_missing: Option<bool>,
}

impl<'a> PhpCall<'a> {
    fn new(row: &QueryResult, call: &'a crate::model::Call) -> Self {
        let package = row.package_of(call).map(|found| found.name);
        let page = row.inertia_page(call);
        let inertia_page_missing = page.as_ref().filter(|page| !page.exists).map(|_| true);
        Self {
            call,
            package,
            inertia_page: page.map(|page| page.rel),
            inertia_page_missing,
        }
    }
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum Comments<'a> {
    Php(&'a [crate::model::Comment]),
    Js(Vec<JsComment<'a>>),
}

/// JS-sorrend: `text` a `tag` előtt.
#[derive(serde::Serialize)]
struct JsComment<'a>(#[serde(serialize_with = "js_comment")] &'a crate::model::Comment);

fn js_comment<S: serde::Serializer>(c: &&crate::model::Comment, ser: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut m = ser.serialize_map(Some(5))?;
    m.serialize_entry("container", &c.container)?;
    m.serialize_entry("caller", &c.caller)?;
    m.serialize_entry("line", &c.line)?;
    m.serialize_entry("text", &c.text)?;
    m.serialize_entry("tag", &c.tag)?;
    m.end()
}

struct JsCall<'a> {
    call: &'a crate::model::Call,
    package: Option<crate::packages::Attribution>,
    /// Laravel-kötésekkel a Ziggy `route('név')` hívásnál a route.
    route: Option<RouteJson>,
    /// Laravel + Inertia kötésekkel a `useForm` küldésnél az űrlap-szerződés.
    form: Option<FormJson>,
}

/// A `form` mező: az űrlap mezői, a szabály-kulcsok, az eltérések és a controller-akció.
#[derive(serde::Serialize)]
struct FormJson {
    fields: Vec<String>,
    rules: Vec<String>,
    unvalidated: Vec<String>,
    missing_required: Vec<String>,
    action: String,
}

impl FormJson {
    /// A hívás űrlapja a Laravel-kötésekből; kötések nélkül vagy nem ellenőrzött küldésnél `None`.
    fn of(row: &QueryResult, call: &crate::model::Call) -> Option<Self> {
        let form = row.laravel.as_ref()?.form_of(&row.graph.file, call)?;
        Some(Self {
            fields: form.fields.clone(),
            rules: form.rules.clone(),
            unvalidated: form.unvalidated.clone(),
            missing_required: form.missing_required.clone(),
            action: form.action(),
        })
    }
}

/// A `route` mező: a megtalált route (`name`, `verb`, `uri`, `action`), vagy `{"missing": true}`, ha a név nem létezik.
#[derive(serde::Serialize)]
#[serde(untagged)]
enum RouteJson {
    Found {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        verb: String,
        uri: String,
        action: String,
    },
    Missing {
        missing: bool,
    },
}

impl RouteJson {
    /// A hívás route-ja a Laravel-kötésekből; kötések nélkül, nem route-hívásnál vagy üres route-táblánál `None`.
    fn of(row: &QueryResult, call: &crate::model::Call) -> Option<Self> {
        let laravel = row.laravel.as_ref()?;
        Some(match laravel.lookup(call)? {
            Some(route) => Self::Found {
                name: route.name.clone(),
                verb: route.verb.clone(),
                uri: route.uri.clone(),
                action: route.action.full(),
            },
            None => Self::Missing { missing: true },
        })
    }
}

impl serde::Serialize for JsCall<'_> {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let c = self.call;
        let mut m = ser.serialize_map(None)?;
        m.serialize_entry("container", &c.container)?;
        m.serialize_entry("caller", &c.caller)?;
        m.serialize_entry("line", &c.line)?;
        if let Some(v) = &c.end_line {
            m.serialize_entry("endLine", v)?;
        }
        m.serialize_entry("type", &c.kind)?;
        m.serialize_entry("target", &c.target)?;
        if let Some(v) = &c.on {
            m.serialize_entry("on", v)?;
        }
        m.serialize_entry("category", &c.category)?;
        if let Some(v) = &c.resolved {
            m.serialize_entry("resolved", v)?;
        }
        if let Some(v) = &c.args {
            m.serialize_entry("args", v)?;
        }
        if let Some(v) = &c.handler {
            m.serialize_entry("handler", v)?;
        }
        // ha a `resolved` már a csomag nevével kezdődik (`vue::computed`), a `package` csak ismétlés lenne
        let repeated = |name: &str| {
            c.resolved.as_deref().is_some_and(|r| {
                r.strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with("::"))
            })
        };
        if let Some(package) = self.package.as_ref().filter(|p| !repeated(&p.name)) {
            m.serialize_entry("package", &package.name)?;
        }
        if let Some(route) = &self.route {
            m.serialize_entry("route", route)?;
        }
        if let Some(form) = &self.form {
            m.serialize_entry("form", form)?;
        }
        m.end()
    }
}

pub(super) fn ordered_graph(row: &QueryResult) -> OrderedGraph<'_> {
    let graph = &row.graph;
    OrderedGraph {
        file: &graph.file,
        module: &graph.module,
        framework: &graph.framework,
        namespace: &graph.namespace,
        imports: &graph.imports,
        packages: graph
            .calls
            .iter()
            .filter_map(|call| row.package_of(call))
            .filter_map(|found| Some((found.name, found.version?)))
            .collect(),
        containers: &graph.containers,
        properties: &graph.properties,
        constants: &graph.constants,
        methods: &graph.methods,
        calls: if graph.module.is_some() {
            Calls::Js(
                graph
                    .calls
                    .iter()
                    .map(|call| JsCall {
                        call,
                        package: row.package_of(call),
                        route: RouteJson::of(row, call),
                        form: FormJson::of(row, call),
                    })
                    .collect(),
            )
        } else if row.packages.is_some() || row.inertia.is_some() {
            Calls::PhpPackages(
                graph
                    .calls
                    .iter()
                    .map(|call| PhpCall::new(row, call))
                    .collect(),
            )
        } else {
            Calls::Php(&graph.calls)
        },
        control_flow: &graph.control_flow,
        strings: &graph.strings,
        comments: if graph.module.is_some() {
            Comments::Js(graph.comments.iter().map(JsComment).collect())
        } else {
            Comments::Php(&graph.comments)
        },
        writes: &graph.writes,
        sfc: &graph.sfc,
        parse_errors: &graph.parse_errors,
        protected: crate::io::is_protected(std::path::Path::new(&graph.file)),
    }
}

pub struct Json;

impl Formatter for Json {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        let graphs: Vec<_> = rows.iter().map(ordered_graph).collect();
        let mut text = serde_json::to_string_pretty(&graphs).unwrap_or_default();
        text.push('\n');
        text
    }
}

pub struct Jsonl;

impl Formatter for Jsonl {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        rows.iter()
            .map(|r| serde_json::to_string(&ordered_graph(r)).unwrap_or_default() + "\n")
            .collect()
    }
}
