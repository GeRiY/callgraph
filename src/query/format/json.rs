//! `json` (a callgraph `jq -s '.'` paritása) és `jsonl` (soronként egy FileGraph) formázók.

use crate::model::FileGraph;
use crate::query::QueryResult;

use super::Formatter;

pub(super) struct OrderedGraph<'a> {
    file: &'a str,
    module: &'a Option<String>,
    framework: &'a Option<String>,
    namespace: &'a Option<String>,
    imports: &'a crate::model::Imports,
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
        m.end()
    }
}

/// A hívások mezősorrendje nyelvfüggő: a JS-motor `category`-t a `resolved` elé teszi.
#[derive(serde::Serialize)]
#[serde(untagged)]
enum Calls<'a> {
    Php(&'a [crate::model::Call]),
    Js(Vec<JsCall<'a>>),
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

struct JsCall<'a>(&'a crate::model::Call);

impl serde::Serialize for JsCall<'_> {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let c = self.0;
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
        m.end()
    }
}

pub(super) fn ordered_graph(graph: &FileGraph) -> OrderedGraph<'_> {
    OrderedGraph {
        file: &graph.file,
        module: &graph.module,
        framework: &graph.framework,
        namespace: &graph.namespace,
        imports: &graph.imports,
        containers: &graph.containers,
        properties: &graph.properties,
        constants: &graph.constants,
        methods: &graph.methods,
        calls: if graph.module.is_some() {
            Calls::Js(graph.calls.iter().map(JsCall).collect())
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
    }
}

pub struct Json;

impl Formatter for Json {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        let graphs: Vec<_> = rows.iter().map(|r| ordered_graph(&r.graph)).collect();
        let mut text = serde_json::to_string_pretty(&graphs).unwrap_or_default();
        text.push('\n');
        text
    }
}

pub struct Jsonl;

impl Formatter for Jsonl {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        rows.iter()
            .map(|r| serde_json::to_string(&ordered_graph(&r.graph)).unwrap_or_default() + "\n")
            .collect()
    }
}
