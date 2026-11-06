//! Az MCP-szerver (rmcp): nyolc, csak olvasó eszköz a meglévő lekérdező-motor fölött. A leírások rövidek (minden
//! kliensben kontextus-tokenbe kerülnek): mit ad, egy példa, a fő buktató. Az `initialize` útmutatója és a `guide`
//! eszköz szövege a skill-utasítás forrásából készül (`guide.rs`), így nem csúszik szét a SKILL.md-vel.

use std::sync::Arc;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};

use super::params::{
    FindCallersParams, FrameworkViewParams, MethodBodyParams, NotesParams, OutlineParams,
    SearchParams,
};
use super::request::ShowQuery;
use super::runner::QueryRunner;

/// Az MCP-kiszolgáló; a lekérdezéseket a befecskendezett `QueryRunner` futtatja.
#[derive(Clone)]
pub(crate) struct CallgraphServer {
    runner: Arc<dyn QueryRunner>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CallgraphServer {
    pub(crate) fn new(runner: Arc<dyn QueryRunner>) -> Self {
        Self {
            runner,
            tool_router: Self::tool_router(),
        }
    }

    /// A lekérdezés a blokkoló szálkészleten fut (elemzés, fájl-I/O), a hiba eszköz-hibaként tér vissza.
    async fn execute(&self, query: Result<ShowQuery, String>) -> CallToolResult {
        let runner = Arc::clone(&self.runner);
        let result = match query {
            Ok(query) => tokio::task::spawn_blocking(move || runner.run(&query))
                .await
                .unwrap_or_else(|e| Err(e.to_string())),
            Err(message) => Err(message),
        };
        to_result(result)
    }

    #[tool(
        description = "Structure of a code file or directory without reading it: classes/components, methods with line ranges, calls by category. With `method`: that one method's call graph. Start here. Example: {\"path\": \"app/Order.php\", \"method\": \"cancel\"}. Pitfall: give the method in `method`, not as `path::method`; on a large directory use `format: summary` or a subdirectory."
    )]
    async fn outline(&self, Parameters(p): Parameters<OutlineParams>) -> CallToolResult {
        self.execute(ShowQuery::outline(p)).await
    }

    #[tool(
        description = "Line-numbered source of one or more methods/functions, compact: the docblock above the method and blank lines dropped, comments inside the body kept (line numbers stay exact; the header says when a docblock was omitted; `doc: true` adds it, `raw: true` = exact, unfiltered text, e.g. before editing). Use after `outline` instead of reading the whole file. Example: {\"path\": \"app/Order.php\", \"method\": [\"cancel\", \"refund\"]} (`method` may be a string or a list)."
    )]
    async fn method_body(&self, Parameters(p): Parameters<MethodBodyParams>) -> CallToolResult {
        self.execute(ShowQuery::method_body(p)).await
    }

    #[tool(
        description = "Who calls it: file, method, line range and the call's own line (`L25 │ code`) of each caller; paths are relative to the `root:` line. Start with the name pattern (regex) `(^|::|\\.|->)cancel$`: it finds every call, also with an unknown/inherited receiver. `Order::cancel` lists only calls resolved to that type (`via <Type>` = through its interface/parent/trait), so it can miss callers; `?::name` = name-only match. `depth` 2–5 = impact analysis (callers of callers); `guess` adds name-only matches (?). Example: {\"pattern\": \"(^|::|\\\\.|->)cancel$\", \"path\": \"app\"}."
    )]
    async fn find_callers(&self, Parameters(p): Parameters<FindCallersParams>) -> CallToolResult {
        self.execute(ShowQuery::find_callers(p)).await
    }

    #[tool(
        description = "Where a string literal (error message, key) or a constant is used/defined (substring match). Example: {\"kind\": \"strings\", \"pattern\": \"Order not found\"} or {\"kind\": \"constants\", \"pattern\": \"STATUS_ACTIVE\"}. For calls of a function use `find_callers`."
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> CallToolResult {
        self.execute(ShowQuery::search(p)).await
    }

    #[tool(
        description = "Framework view of a directory: routes, middleware, validation, jobs, events, providers, magic, di, views. Example: {\"view\": \"routes\", \"path\": \"routes\"}. Run it on the directory that holds the definitions (a file works too)."
    )]
    async fn framework_view(
        &self,
        Parameters(p): Parameters<FrameworkViewParams>,
    ) -> CallToolResult {
        self.execute(ShowQuery::framework_view(p)).await
    }

    #[tool(
        description = "Notes (pilot) attached to files or methods by people/agents: `path[::method]  time  📝 text`, [orphaned] if the method is gone. Read-only (notes are written with the CLI `callgraph note add`). Example: {\"pattern\": \"refund\", \"path\": \"app\"}; without `pattern`: all notes."
    )]
    async fn notes(&self, Parameters(p): Parameters<NotesParams>) -> CallToolResult {
        self.execute(ShowQuery::notes(p)).await
    }

    #[tool(
        description = "Registered project aliases and their roots (for the `project` parameter). Call it when the server's working directory is not the project (e.g. Claude Desktop), then pass `project` to every tool."
    )]
    async fn projects(&self) -> CallToolResult {
        let runner = Arc::clone(&self.runner);
        let result = tokio::task::spawn_blocking(move || runner.projects())
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        to_result(result)
    }

    #[tool(
        description = "The full callgraph manual (the skill's SKILL.md in the configured language): every query with CLI examples, JS/TS/Vue, Rust and Swift specifics, formats, limits. Call it once when the server instructions are not enough; the CLI flags map to these tools (see its MCP section)."
    )]
    async fn guide(&self) -> CallToolResult {
        to_result(Ok(super::guide::manual(crate::i18n::lang())))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CallgraphServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("callgraph", env!("CARGO_PKG_VERSION")))
            .with_instructions(super::guide::instructions(crate::i18n::lang()))
    }
}

/// Siker: a CLI-vel azonos szöveg; hiba: `isError` eszköz-eredmény a kétnyelvű üzenettel (nem protokoll-hiba).
fn to_result(result: Result<String, String>) -> CallToolResult {
    match result {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
    }
}
