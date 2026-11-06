//! Az MCP-szerver (rmcp): kilenc, csak olvasó eszköz a meglévő lekérdező-motor fölött. A leírások rövidek (minden
//! kliensben kontextus-tokenbe kerülnek): mit ad, egy példa, a fő buktató. Az `initialize` útmutatója és a `guide`
//! eszköz szövege a skill-utasítás forrásából készül (`guide.rs`), így nem csúszik szét a SKILL.md-vel. Minden
//! eszköz-hívás vége egy helyen megy át (`finish`): ott méri a helyi használati statisztika (`stats::McpStats`).

use std::sync::Arc;
use std::time::Instant;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};

use super::params::{
    FindCallersParams, FrameworkViewParams, LocateParams, MethodBodyParams, NotesParams,
    OutlineParams, ProfileParams, SearchParams,
};
use super::request::ShowQuery;
use super::runner::QueryRunner;
use crate::stats::{CommandKind, McpStats, ToolCall};

/// Az MCP-kiszolgáló; a lekérdezéseket a befecskendezett `QueryRunner` futtatja, a hívásokat a `McpStats` méri.
#[derive(Clone)]
pub(crate) struct CallgraphServer {
    runner: Arc<dyn QueryRunner>,
    stats: Arc<McpStats>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CallgraphServer {
    pub(crate) fn new(runner: Arc<dyn QueryRunner>, stats: McpStats) -> Self {
        Self {
            runner,
            stats: Arc::new(stats),
            tool_router: Self::tool_router(),
        }
    }

    /// A lekérdezés a blokkoló szálkészleten fut (elemzés, fájl-I/O), a hiba eszköz-hibaként tér vissza.
    async fn execute(&self, tool: CommandKind, query: Result<ShowQuery, String>) -> CallToolResult {
        let started = Instant::now();
        let target = query
            .as_ref()
            .ok()
            .map(|q| (q.project.clone(), q.path().to_owned()));
        let (project, path) = target.unzip();
        let runner = Arc::clone(&self.runner);
        let invalid = query.is_err();
        let result = match query {
            Ok(query) => tokio::task::spawn_blocking(move || runner.run(&query))
                .await
                .unwrap_or_else(|e| Err(e.to_string())),
            Err(message) => Err(message),
        };
        let project = project.flatten();
        let call = ToolCall {
            tool,
            started,
            project: project.as_deref(),
            path: path.as_deref(),
            invalid,
        };
        self.finish(&call, result)
    }

    /// Minden eszköz-hívás vége: a helyi mérés (kikapcsolva semmi), majd az eszköz-eredmény.
    fn finish(&self, call: &ToolCall<'_>, result: Result<String, String>) -> CallToolResult {
        self.stats.tool_call(call, &result);
        to_result(result)
    }

    /// Egy cél nélküli hívás (`projects`, `project_profile`, `guide`) leírása.
    fn plain(tool: CommandKind, started: Instant, project: Option<&str>) -> ToolCall<'_> {
        ToolCall {
            tool,
            started,
            project,
            path: None,
            invalid: false,
        }
    }

    #[tool(
        description = "Where to start with a task: give the names and domain terms from the task in ONE call; answers from the project map (no file reading) with a short ranked list of files (top 5; `all: true` = every match), each with its reasons: exact definition, file name, name part, string literal, call, `callers: N`. Example: {\"terms\": [\"RefundController\", \"processRefund\", \"refund\", \"payment_status\", \"checkout.vue\"]}. Pitfall: pass code names, not a sentence; translate non-English domain words to their English code names (visszatérítés → refund); one call with all terms, not one call per term. For exact text (log message, config value) use Grep."
    )]
    async fn locate(&self, Parameters(p): Parameters<LocateParams>) -> CallToolResult {
        self.execute(CommandKind::Locate, ShowQuery::locate(p))
            .await
    }

    #[tool(
        description = "Structure of a code file or directory without reading it: classes/components, methods with line ranges, calls by category. With `method`: that one method's call graph. Start here once you know the file (else `locate`). Example: {\"path\": \"app/Order.php\", \"method\": \"cancel\"}. Pitfall: give the method in `method`, not as `path::method`; on a large directory use `format: summary` or a subdirectory."
    )]
    async fn outline(&self, Parameters(p): Parameters<OutlineParams>) -> CallToolResult {
        self.execute(CommandKind::Outline, ShowQuery::outline(p))
            .await
    }

    #[tool(
        description = "Line-numbered source of one or more methods/functions, compact: the docblock above the method and blank lines dropped, comments inside the body kept (line numbers stay exact; the header says when a docblock was omitted; `doc: true` adds it, `raw: true` = exact, unfiltered text, e.g. before editing). Use after `outline` instead of reading the whole file. Example: {\"path\": \"app/Order.php\", \"method\": [\"cancel\", \"refund\"]} (`method` may be a string or a list)."
    )]
    async fn method_body(&self, Parameters(p): Parameters<MethodBodyParams>) -> CallToolResult {
        self.execute(CommandKind::MethodBody, ShowQuery::method_body(p))
            .await
    }

    #[tool(
        description = "Who calls it: file, method, line range and the call's own line (`L25 │ code`) of each caller; paths are relative to the `root:` line. Start with the name pattern (regex) `(^|::|\\.|->)cancel$`: it finds every call, also with an unknown/inherited receiver. `Order::cancel` lists only calls resolved to that type (`via <Type>` = through its interface/parent/trait), so it can miss callers; `?::name` = name-only match. `depth` 2–5 = impact analysis (callers of callers); `guess` adds name-only matches (?). Example: {\"pattern\": \"(^|::|\\\\.|->)cancel$\", \"path\": \"app\"}."
    )]
    async fn find_callers(&self, Parameters(p): Parameters<FindCallersParams>) -> CallToolResult {
        self.execute(CommandKind::FindCallers, ShowQuery::find_callers(p))
            .await
    }

    #[tool(
        description = "Where a string literal (error message, key) or a constant is used/defined (substring match). Example: {\"kind\": \"strings\", \"pattern\": \"Order not found\"} or {\"kind\": \"constants\", \"pattern\": \"STATUS_ACTIVE\"}. For calls of a function use `find_callers`."
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> CallToolResult {
        self.execute(CommandKind::Search, ShowQuery::search(p))
            .await
    }

    #[tool(
        description = "Framework view of a directory: routes, middleware, validation, jobs, events, providers, magic, di, views. Example: {\"view\": \"routes\", \"path\": \"routes\"}. Run it on the directory that holds the definitions (a file works too)."
    )]
    async fn framework_view(
        &self,
        Parameters(p): Parameters<FrameworkViewParams>,
    ) -> CallToolResult {
        self.execute(CommandKind::FrameworkView, ShowQuery::framework_view(p))
            .await
    }

    #[tool(
        description = "Notes (pilot) attached to files or methods by people/agents: `path[::method]  time  📝 text`, [orphaned] if the method is gone. Read-only (notes are written with the CLI `callgraph note add`). Example: {\"pattern\": \"refund\", \"path\": \"app\"}; without `pattern`: all notes."
    )]
    async fn notes(&self, Parameters(p): Parameters<NotesParams>) -> CallToolResult {
        self.execute(CommandKind::Notes, ShowQuery::notes(p)).await
    }

    #[tool(
        description = "Registered project aliases and their roots (for the `project` parameter). Call it when the server's working directory is not the project (e.g. Claude Desktop), then pass `project` to every tool."
    )]
    async fn projects(&self) -> CallToolResult {
        let started = Instant::now();
        let runner = Arc::clone(&self.runner);
        let result = tokio::task::spawn_blocking(move || runner.projects())
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        self.finish(&Self::plain(CommandKind::Projects, started, None), result)
    }

    #[tool(
        description = "Project profile from folder and file names, checked against the code graph: architecture (Laravel/Yii layered MVC, DDD/hexagonal, Vue/React frontend, SwiftUI MVVM, Rust web), layers with their folders and file counts, design patterns, the call direction between layers, unknown folders (~15 lines). Call it first in an unfamiliar project, then pass `layer` (e.g. \"service\") to `outline` / `search` / `find_callers`. Example: {} or {\"project\": \"shop\", \"full\": true}. Pitfall: a layer name comes from this list (controller, service, repository, model, component, store…), not a folder path."
    )]
    async fn project_profile(&self, Parameters(p): Parameters<ProfileParams>) -> CallToolResult {
        let started = Instant::now();
        let project = p.project.clone();
        let runner = Arc::clone(&self.runner);
        let result =
            tokio::task::spawn_blocking(move || runner.profile(p.project.as_deref(), p.full))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
        let call = Self::plain(CommandKind::ProjectProfile, started, project.as_deref());
        self.finish(&call, result)
    }

    #[tool(
        description = "The full callgraph manual (the skill's SKILL.md in the configured language): every query with CLI examples, JS/TS/Vue, Rust and Swift specifics, formats, limits. Call it once when the server instructions are not enough; the CLI flags map to these tools (see its MCP section)."
    )]
    async fn guide(&self) -> CallToolResult {
        let started = Instant::now();
        let result = Ok(super::guide::manual(crate::i18n::lang()));
        self.finish(&Self::plain(CommandKind::Guide, started, None), result)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CallgraphServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("callgraph", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                super::guide::instructions(crate::i18n::lang()) + &self.runner.startup_note(),
            )
    }
}

/// Siker: a CLI-vel azonos szöveg; hiba: `isError` eszköz-eredmény a kétnyelvű üzenettel (nem protokoll-hiba).
fn to_result(result: Result<String, String>) -> CallToolResult {
    match result {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
    }
}
