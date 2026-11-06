//! Az MCP-eszközök bemenete (serde + JSON Schema). A mezők doc-kommentje a sémában leírásként jelenik meg
//! (minden kliensnél kontextus-token): rövid, pontos, angol.

use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;

/// Az `outline` eszköz nézetei (a `show --format` egy része).
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

/// A `search` eszköz keresési módja.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchKind {
    Strings,
    Constants,
}

/// A framework-nézetek (a `show --format` framework-formátumai).
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
}

/// `outline`: egy fájl / mappa szerkezete vagy egy metódus hívás-gráfja.
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
    /// Disable the 400-line output cap (use only when needed).
    #[serde(default)]
    pub full: bool,
}

/// Egy név vagy névlista: az LLM-kliensek gyakran sima stringet küldenek tömb helyett (`"method": "parse"`).
/// A séma `anyOf` string | string-tömb.
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

/// `method_body`: egy vagy több metódus forrása sorszámozva.
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

/// `find_callers`: hívók / hatáselemzés.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct FindCallersParams {
    /// What is called (regex on the callee): start with `(^|::|\.|->)cancel$` (every call of the name);
    /// `Order::cancel` = only calls resolved to that type; JS keys: `useForm.js::submit`, `route::orders.show`.
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
    /// Disable the 400-line output cap.
    #[serde(default)]
    pub full: bool,
}

/// `search`: string-literál vagy konstans keresése.
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
    /// Disable the 400-line output cap.
    #[serde(default)]
    pub full: bool,
}

/// `framework_view`: framework-nézet egy mappára.
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

/// `notes` (pilot): a fájlokhoz / metódusokhoz írt jegyzetek keresése (csak olvas).
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

/// `locate`: kiindulópont-kereső — a feladat nevei és fogalmai egy hívásban.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct LocateParams {
    /// Names and domain terms from the task, in code form: function / class / file names, columns, routes, and the
    /// English code name of a non-English domain word (`["RefundController", "processRefund", "refund",
    /// "payment_status", "checkout.vue"]`); a single string works too.
    pub terms: OneOrMany,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// Every matching file instead of the top 5.
    #[serde(default)]
    pub all: bool,
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
    fn method_schema_allows_string_and_array() {
        let schema = serde_json::to_string(&schemars::schema_for!(MethodBodyParams)).unwrap();
        assert!(schema.contains("anyOf"), "{schema}");
        assert!(schema.contains("\"array\""), "{schema}");
    }
}
