//! Az MCP-szerver (rmcp): hét, csak olvasó eszköz a meglévő lekérdező-motor fölött. A leírások rövidek (minden
//! kliensben kontextus-tokenbe kerülnek), a `skill/SKILL.md` útmutatását követik.

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

/// A szerver általános útmutatója (az `initialize` válaszában).
const INSTRUCTIONS: &str = "callgraph: AST-based, token-efficient code map (PHP, Rust, JS/TS/Vue, Swift, SCSS). \
Use it before reading a code file: `outline` first, then `method_body` for just the method you need, \
`find_callers` for who-calls-it / impact (depth). Paths are relative to the project root: a registered alias \
(`project`, see `projects`) or the server's working directory. Output is capped at 400 lines (it switches to a \
summary / method index); narrow the path or method instead of `full: true`. Read-only.";

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
        description = "Structure of a code file or directory without reading it: classes/components, methods with line ranges, calls by category. With `method`: that one method's call graph. Start here."
    )]
    async fn outline(&self, Parameters(p): Parameters<OutlineParams>) -> CallToolResult {
        self.execute(ShowQuery::outline(p)).await
    }

    #[tool(
        description = "Line-numbered source of one or more methods/functions, compact: docblock, blank and comment-only lines dropped (line numbers stay exact; `doc: true` adds the docblock, `raw: true` = unfiltered). Use after `outline` instead of reading the whole file."
    )]
    async fn method_body(&self, Parameters(p): Parameters<MethodBodyParams>) -> CallToolResult {
        self.execute(ShowQuery::method_body(p)).await
    }

    #[tool(
        description = "Who calls it: file, method, line range of each caller. `depth` 2–5 = impact analysis (callers of callers); `guess` adds name-only matches (?)."
    )]
    async fn find_callers(&self, Parameters(p): Parameters<FindCallersParams>) -> CallToolResult {
        self.execute(ShowQuery::find_callers(p)).await
    }

    #[tool(
        description = "Where a string literal (error message, key) or a constant is used/defined."
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> CallToolResult {
        self.execute(ShowQuery::search(p)).await
    }

    #[tool(
        description = "Framework view of a directory: routes, middleware, validation, jobs, events, providers, magic, di, views."
    )]
    async fn framework_view(
        &self,
        Parameters(p): Parameters<FrameworkViewParams>,
    ) -> CallToolResult {
        self.execute(ShowQuery::framework_view(p)).await
    }

    #[tool(
        description = "Notes (pilot) attached to files or methods by people/agents: `path[::method]  time  📝 text`, [orphaned] if the method is gone. Read-only (notes are written with the CLI `callgraph note add`)."
    )]
    async fn notes(&self, Parameters(p): Parameters<NotesParams>) -> CallToolResult {
        self.execute(ShowQuery::notes(p)).await
    }

    #[tool(
        description = "Registered project aliases and their roots (for the `project` parameter)."
    )]
    async fn projects(&self) -> CallToolResult {
        let runner = Arc::clone(&self.runner);
        let result = tokio::task::spawn_blocking(move || runner.projects())
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        to_result(result)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CallgraphServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("callgraph", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Siker: a CLI-vel azonos szöveg; hiba: `isError` eszköz-eredmény a kétnyelvű üzenettel (nem protokoll-hiba).
fn to_result(result: Result<String, String>) -> CallToolResult {
    match result {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
    }
}
