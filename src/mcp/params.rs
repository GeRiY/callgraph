//! Az MCP-eszközök bemenete (serde + JSON Schema). A mezők doc-kommentje a sémában leírásként jelenik meg
//! (minden kliensnél kontextus-token): rövid, pontos, angol. A típusok fölött szándékosan sima `//` megjegyzés áll:
//! a `///` a séma tetejére (és minden hivatkozó eszköz `$defs`-ébe) is bekerülne, a `tools/list` hosszát növelve.

use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;

// Az `outline` eszköz nézetei (a `show --format` egy része).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OutlineFormat {
    #[default]
    Outline,
    Api,
    Flow,
    Summary,
    Hotspots,
    Stats,
}

// A `search` eszköz keresési módja.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchKind {
    Strings,
    Constants,
}

// A framework-nézetek (a `show --format` framework-formátumai).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FrameworkView {
    Routes,
    Middleware,
    Validation,
    Jobs,
    Events,
    Providers,
    Magic,
    Di,
    Views,
    Inertia,
    Ziggy,
    Forms,
}

// `outline`: egy fájl / mappa szerkezete vagy egy metódus hívás-gráfja.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct OutlineParams {
    /// File or directory, relative to the project root.
    pub path: String,
    /// Registered project alias (see `projects`); default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// outline (default): classes, methods with line ranges, calls by category; api: public surface;
    /// flow: control branches; summary: one line per file; hotspots: most-called; stats: counts.
    #[serde(default)]
    pub format: Option<OutlineFormat>,
    /// Only this method/function (file paths only).
    #[serde(default)]
    pub method: Option<String>,
    /// Only files of this layer (see `project_profile`), e.g. `service`, `service,repository`, or one ecosystem:
    /// `controller:js`.
    #[serde(default)]
    pub layer: Option<String>,
    /// Disable the 400-line output cap (use only when needed).
    #[serde(default)]
    pub full: bool,
}

// Egy név vagy névlista: az LLM-kliensek gyakran sima stringet küldenek tömb helyett (`"method": "parse"`).
// A séma `anyOf` string | string-tömb.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl Default for OneOrMany {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl OneOrMany {
    pub(crate) fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(name) => vec![name],
            Self::Many(names) => names,
        }
    }
}

// `method_body`: egy vagy több metódus forrása sorszámozva.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct MethodBodyParams {
    /// File path, relative to the project root.
    pub path: String,
    /// Method/function name (`"cancel"`), or an array of names (`["cancel", "refund"]`).
    pub method: OneOrMany,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// Also include the docblock / doc comments above each method.
    #[serde(default)]
    pub doc: bool,
    /// Exact, unfiltered source (docblock and blank lines kept), e.g. before an exact edit.
    #[serde(default)]
    pub raw: bool,
    /// Disable the 400-line output cap.
    #[serde(default)]
    pub full: bool,
}

// `find_callers`: hívók / hatáselemzés.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct FindCallersParams {
    /// Regex on the callee: `(^|::|\.|->)cancel$` (every call), `Order::cancel` (resolved to that type); JS keys:
    /// `useForm.js::submit`, `route::orders.show`.
    pub pattern: String,
    /// Search scope (file or directory) relative to the project root; default: the whole root.
    #[serde(default)]
    pub path: Option<String>,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// 1 (default) = direct callers; 2–5 = callers of callers (impact analysis).
    #[serde(default)]
    pub depth: Option<u8>,
    /// Also follow name-only matches (marked `?`).
    #[serde(default)]
    pub guess: bool,
    /// Search only files of this layer (level 1), e.g. `controller`.
    #[serde(default)]
    pub layer: Option<String>,
    /// Group callers by layer (closest to the callee first).
    #[serde(default)]
    pub by_layer: bool,
    /// Disable the 400-line output cap.
    #[serde(default)]
    pub full: bool,
}

// `search`: string-literál vagy konstans keresése.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct SearchParams {
    /// strings: string literals (messages, keys); constants: constant references and definitions.
    pub kind: SearchKind,
    /// Text to look for (substring).
    pub pattern: String,
    /// Search scope relative to the project root; default: the whole root.
    #[serde(default)]
    pub path: Option<String>,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// Only files of this layer, e.g. `service`.
    #[serde(default)]
    pub layer: Option<String>,
    /// Disable the 400-line output cap.
    #[serde(default)]
    pub full: bool,
}

// `framework_view`: framework-nézet egy mappára.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct FrameworkViewParams {
    /// Which view.
    pub view: FrameworkView,
    /// Directory (or file) relative to the project root, e.g. `routes`, `app`.
    pub path: String,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// Disable the 400-line output cap.
    #[serde(default)]
    pub full: bool,
}

// `notes` (pilot): a fájlokhoz / metódusokhoz írt jegyzetek keresése (csak olvas).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct NotesParams {
    /// Regex on the note text or its `path::method` anchor (case-insensitive); default: all notes.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Only notes under this file or directory (relative to the project root); default: the whole root.
    #[serde(default)]
    pub path: Option<String>,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
}

// `locate`: kiindulópont-kereső — a feladat nevei és fogalmai egy hívásban.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct LocateParams {
    /// The task's names and terms in code form (function / class / file names, columns, routes, English code names
    /// of non-English words); a string works too. An element with spaces = one phrase matched against string
    /// literals (a UI label); `route::events.show` = a route's definition and `route()` calls; `--orange-500` = a
    /// CSS custom property.
    pub terms: OneOrMany,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// Every matching file instead of the top 5.
    #[serde(default)]
    pub all: bool,
}

// `generate` (opt-in, `--generate` / `CALLGRAPH_GENERATE=1`): PHP-kód generálása rövid kifejezésből. (Típus-szintű
// `//` komment: a `///` a sémába kerülne, és növelné a `tools/list` tokenjeit.)
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct GenerateParams {
    /// The expression, e.g. `c!:Pay<Base>[Chargeable] > ctor(-Client $http) + +charge(Order $o):Receipt{ … }`:
    /// `c:` class `i:` interface `t:` trait `e:` enum; `>` child, `+` sibling, `^` up, `*n` repeat; members
    /// `+`/`-`/`#` visibility, `::` static, `name(Type $p):Type{body}`, `$prop:Type=1`, `K=1`, `ctor(-Type $x)` promoted.
    pub expr: String,
    /// Return the generated code instead of writing it.
    #[serde(default)]
    pub dry: bool,
    /// New file to create (an existing file is never overwritten).
    #[serde(default)]
    pub write: Option<String>,
    /// `path` or `path::Class`: insert the generated members into that class.
    #[serde(default)]
    pub into: Option<String>,
    /// With `into`: insert after this method (default: at the end of the class).
    #[serde(default)]
    pub after: Option<String>,
    /// `path::method` or `path:L42-L87`: replace that method / line range (a backup is made first).
    #[serde(default)]
    pub replace: Option<String>,
    /// With `replace`: refuse unless the replaced text still has this sha256 prefix.
    #[serde(default)]
    pub expect: Option<String>,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
}

impl GenerateParams {
    /// A CLI `gen` argumentumai (a kifejezés külön argumentum; a `--project` a hívónál jön).
    pub(crate) fn to_args(&self) -> Vec<String> {
        let mut args = vec!["gen".to_owned(), self.expr.clone()];
        if self.dry {
            args.push("--dry".to_owned());
        }
        for (flag, value) in [
            ("--write", &self.write),
            ("--into", &self.into),
            ("--after", &self.after),
            ("--replace", &self.replace),
            ("--expect", &self.expect),
        ] {
            if let Some(value) = value {
                args.push(flag.to_owned());
                args.push(value.clone());
            }
        }
        args
    }
}

// `next`: egy korábbi eredmény javasolt következő lépése az azonosítójával.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct NextParams {
    /// The result id from the `↪ rN …` line, e.g. `"r7"`.
    pub result_id: String,
    /// Which suggestion(s): `1`, `"1"`, `"1,3"` or `[1, 3]`.
    pub choice: Choice,
    /// Registered project alias (the one the result came from); default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
}

// Egy vagy több választás: szám, string (`"1,3"`) vagy tömb.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum Choice {
    Number(u64),
    Text(String),
    List(Vec<ChoiceItem>),
}

// A tömb egy eleme: szám vagy string.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum ChoiceItem {
    Number(u64),
    Text(String),
}

impl Default for Choice {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl Choice {
    /// A `next` alak: `1,3`.
    pub(crate) fn to_text(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Text(text) => text.clone(),
            Self::List(items) => items
                .iter()
                .map(|item| match item {
                    ChoiceItem::Number(n) => n.to_string(),
                    ChoiceItem::Text(text) => text.clone(),
                })
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

// `project_profile`: a projekt architektúrája, rétegei és mintái.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct ProfileParams {
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// Per-folder details, confidence counts and the call pairs between layers.
    #[serde(default)]
    pub full: bool,
    /// Also list every direct composer / npm dependency: installed and declared version, dev flag.
    #[serde(default)]
    pub packages: bool,
    /// Instead: probably unused, missing and dev-only composer / npm dependencies.
    #[serde(default)]
    pub deps: bool,
}

// `package_usage`: egy függőség használata (a CLI `uses`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct PackageUsageParams {
    /// Package name or short name: `guzzlehttp/guzzle`, `guzzle`, `@vueuse/core`.
    pub package: String,
    /// Every call line per file instead of the per-symbol summary.
    #[serde(default)]
    pub calls: bool,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
}

impl OutlineFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Outline => "outline",
            Self::Api => "api",
            Self::Flow => "flow",
            Self::Summary => "summary",
            Self::Hotspots => "hotspots",
            Self::Stats => "stats",
        }
    }
}

impl FrameworkView {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Routes => "routes",
            Self::Middleware => "middleware",
            Self::Validation => "validation",
            Self::Jobs => "jobs",
            Self::Events => "events",
            Self::Providers => "providers",
            Self::Magic => "magic",
            Self::Di => "di",
            Self::Views => "views",
            Self::Inertia => "inertia",
            Self::Ziggy => "ziggy",
            Self::Forms => "forms",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_accepts_a_string_or_an_array() {
        let one: MethodBodyParams =
            serde_json::from_value(serde_json::json!({"path": "a.rs", "method": "parse"})).unwrap();
        assert_eq!(one.method.into_vec(), vec!["parse".to_owned()]);
        let many: MethodBodyParams = serde_json::from_value(
            serde_json::json!({"path": "a.rs", "method": ["a", "b"], "doc": true}),
        )
        .unwrap();
        assert!(many.doc && !many.raw);
        assert_eq!(many.method.into_vec(), vec!["a".to_owned(), "b".to_owned()]);
        let wrong = serde_json::json!({"path": "a.rs", "method": 3});
        assert!(serde_json::from_value::<MethodBodyParams>(wrong).is_err());
    }

    #[test]
    fn locate_terms_accept_a_string_or_an_array() {
        let one: LocateParams =
            serde_json::from_value(serde_json::json!({"terms": "refund"})).unwrap();
        assert_eq!(one.terms.into_vec(), vec!["refund".to_owned()]);
        let many: LocateParams = serde_json::from_value(
            serde_json::json!({"terms": ["refund", "checkout.vue"], "all": true}),
        )
        .unwrap();
        assert!(many.all && many.project.is_none());
        assert_eq!(many.terms.into_vec().len(), 2);
        assert!(serde_json::from_value::<LocateParams>(serde_json::json!({})).is_err());
    }

    #[test]
    fn package_usage_needs_a_package_and_defaults_to_the_summary() {
        let plain: PackageUsageParams =
            serde_json::from_value(serde_json::json!({"package": "guzzle"})).unwrap();
        assert!(!plain.calls && plain.project.is_none());
        let calls: PackageUsageParams = serde_json::from_value(
            serde_json::json!({"package": "@vueuse/core", "calls": true, "project": "shop"}),
        )
        .unwrap();
        assert!(calls.calls);
        assert_eq!(calls.project.as_deref(), Some("shop"));
        assert!(serde_json::from_value::<PackageUsageParams>(serde_json::json!({})).is_err());
    }

    #[test]
    fn profile_deps_is_an_optional_flag_with_a_short_doc() {
        let plain: ProfileParams = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!plain.deps && !plain.packages);
        let deps: ProfileParams =
            serde_json::from_value(serde_json::json!({"deps": true, "project": "shop"})).unwrap();
        assert!(deps.deps);
        // a `tools/list` alig nő: egy rövid mező-leírás
        let schema = serde_json::to_string(&schemars::schema_for!(ProfileParams)).unwrap();
        let doc = "Instead: probably unused, missing and dev-only composer / npm dependencies.";
        assert!(doc.chars().count() <= 80, "{doc}");
        assert!(schema.contains(doc), "{schema}");
    }

    /// A típusok fölötti (magyar) megjegyzés nem kerül a sémába: csak a mezők angol leírása (`tools/list` hossza).
    #[test]
    fn type_comments_stay_out_of_the_schema() {
        for schema in [
            serde_json::to_string(&schemars::schema_for!(OutlineParams)).unwrap(),
            serde_json::to_string(&schemars::schema_for!(MethodBodyParams)).unwrap(),
            serde_json::to_string(&schemars::schema_for!(NextParams)).unwrap(),
            serde_json::to_string(&schemars::schema_for!(PackageUsageParams)).unwrap(),
            serde_json::to_string(&schemars::schema_for!(GenerateParams)).unwrap(),
        ] {
            for hungarian in ["eszköz", "nézet", "választás", "metódus", "függőség"] {
                assert!(!schema.contains(hungarian), "{hungarian}: {schema}");
            }
        }
    }

    #[test]
    fn method_schema_allows_string_and_array() {
        let schema = serde_json::to_string(&schemars::schema_for!(MethodBodyParams)).unwrap();
        assert!(schema.contains("anyOf"), "{schema}");
        assert!(schema.contains("\"array\""), "{schema}");
    }
}
