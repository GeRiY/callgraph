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

/// `method_body`: egy vagy több metódus forrása sorszámozva.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct MethodBodyParams {
    /// File path, relative to the project root.
    pub path: String,
    /// Method/function names (one or more).
    pub method: Vec<String>,
    /// Registered project alias; default: the server's working directory.
    #[serde(default)]
    pub project: Option<String>,
    /// Disable the 400-line output cap.
    #[serde(default)]
    pub full: bool,
}

/// `find_callers`: hívók / hatáselemzés.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct FindCallersParams {
    /// What is called, e.g. `Order::cancel`, `useForm.js::submit`, `route::orders.show`, `Repo::find`.
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
