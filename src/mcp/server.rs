//! Az MCP-szerver (rmcp): csak olvasó eszközök a meglévő lekérdező-motor fölött, plusz az opt-in, író `generate`
//! (`--generate` / `CALLGRAPH_GENERATE=1`; nélküle nincs a listában). A leírások rövidek (minden kliensben
//! kontextus-tokenbe kerülnek; a `tools/list` teljes JSON-ja ne nőjön): mit ad, egy példa, a fő buktató, mikor melyik
//! eszköz. Az `initialize` útmutatója és a `guide`
//! eszköz szövege a skill-utasítás forrásából készül (`guide.rs`), így nem csúszik szét a SKILL.md-vel. Minden
//! eszköz-hívás vége egy helyen megy át (`finish`): ott méri a helyi használati statisztika (`stats::McpStats`),
//! és ott indul a napi névtelen check (`update::usage`).

use std::sync::Arc;
use std::time::Instant;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};

use super::params::{
    FindCallersParams, FrameworkViewParams, GenerateParams, LocateParams, MethodBodyParams,
    NextParams, NotesParams, OutlineParams, PackageUsageParams, ProfileParams, SearchParams,
};
use super::request::ShowQuery;
use super::runner::QueryRunner;
use crate::stats::{CommandKind, McpStats, ToolCall};
use crate::update::usage::{UsageCheck, Via};

/// Az MCP-kiszolgáló; a lekérdezéseket a befecskendezett `QueryRunner` futtatja, a hívásokat a `McpStats` méri.
#[derive(Clone)]
pub(crate) struct CallgraphServer {
    runner: Arc<dyn QueryRunner>,
    stats: Arc<McpStats>,
    /// A napi névtelen check (alapból kikapcsolva; a valódi szerver a `with_usage`-dzsel kapcsolja be).
    usage: Arc<UsageCheck>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CallgraphServer {
    pub(crate) fn new(runner: Arc<dyn QueryRunner>, stats: McpStats) -> Self {
        let mut tool_router = Self::tool_router();
        // az egyetlen író eszköz alapból nincs a listán (a „csak olvas” ígéret a meglévő telepítéseknél változatlan)
        if !crate::generate::enabled() {
            tool_router.remove_route("generate");
        }
        Self {
            runner,
            stats: Arc::new(stats),
            usage: Arc::new(UsageCheck::disabled()),
            tool_router,
        }
    }

    /// A napi névtelen check bekapcsolása (a nap első eszközhívásakor, háttérszálon; lásd `update::usage`).
    pub(crate) fn with_usage(mut self, usage: UsageCheck) -> Self {
        self.usage = Arc::new(usage);
        self
    }

    /// A lekérdezés a blokkoló szálkészleten fut (elemzés, fájl-I/O), a hiba eszköz-hibaként tér vissza.
    async fn execute(&self, tool: CommandKind, query: Result<ShowQuery, String>) -> CallToolResult {
        let started = Instant::now();
        let target = query
            .as_ref()
            .ok()
            .map(|q| (q.project.clone(), q.path().to_owned()));
        let (project, path) = target.unzip();
        let symbol = query.as_ref().ok().and_then(ShowQuery::symbol);
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
            symbol: symbol.as_deref(),
            invalid,
        };
        self.finish(&call, result)
    }

    /// Minden eszköz-hívás vége: a helyi mérés (kikapcsolva semmi), majd az eszköz-eredmény.
    fn finish(&self, call: &ToolCall<'_>, result: Result<String, String>) -> CallToolResult {
        self.stats.tool_call(call, &result);
        self.usage
            .after_use(crate::update::check::now_secs(), Via::Mcp);
        to_result(result)
    }

    /// Egy cél nélküli hívás (`projects`, `project_profile`, `package_usage`, `guide`) leírása.
    fn plain(tool: CommandKind, started: Instant, project: Option<&str>) -> ToolCall<'_> {
        ToolCall {
            tool,
            started,
            project,
            path: None,
            symbol: None,
            invalid: false,
        }
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Where to start with a task: the task's names and domain terms in ONE call → a ranked file list from the project map (no file reading; top 5, `all: true` = all), each with reasons (exact definition, file name, name part, string literal, call, `callers: N`). Example: {\"terms\": [\"RefundController\", \"refund\", \"payment_status\"]}. Pitfall: code names, not a sentence; translate non-English domain words (visszatérítés → refund); one call with all terms. Use `locate` to find WHERE to start; once you know the file use `outline`; string literal / constant: `search`; other exact text (log message, config value): Grep."
    )]
    async fn locate(&self, Parameters(p): Parameters<LocateParams>) -> CallToolResult {
        self.execute(CommandKind::Locate, ShowQuery::locate(p))
            .await
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Structure of a code file or directory without reading it: classes/components, methods with line ranges, calls by category. With `method`: that method's callees, each classified (internal_helper, model_use, orm_relation, framework_use, external_call…) — no separate callees tool; who calls it: `find_callers`. Start here once you know the file (else `locate`); routes / events / validation: `framework_view`. Example: {\"path\": \"app/Order.php\", \"method\": \"cancel\"}. Pitfall: the method goes in `method`, not `path::method`; on a large directory use `format: summary` or a subdirectory."
    )]
    async fn outline(&self, Parameters(p): Parameters<OutlineParams>) -> CallToolResult {
        self.execute(CommandKind::Outline, ShowQuery::outline(p))
            .await
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Line-numbered source of methods/functions, compact: docblock and blank lines dropped, comments in the body kept (line numbers stay exact; the header notes an omitted docblock; `doc: true` adds it, `raw: true` = exact text, e.g. before editing). Use after `outline` instead of reading the whole file. Example: {\"path\": \"app/Order.php\", \"method\": [\"cancel\", \"refund\"]}."
    )]
    async fn method_body(&self, Parameters(p): Parameters<MethodBodyParams>) -> CallToolResult {
        self.execute(CommandKind::MethodBody, ShowQuery::method_body(p))
            .await
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Who calls it: each caller's file, method, line range and call line (`L25 │ code`); paths relative to the `root:` line. Start with the name regex `(^|::|\\.|->)cancel$`: every call, also with an unknown/inherited receiver. `Order::cancel` = only calls resolved to that type (`via <Type>` = through interface/parent/trait), so it can miss callers; `?::name` = name-only match. `depth` 2–5 = impact analysis (callers of callers); `guess` adds name-only matches (?). Example: {\"pattern\": \"(^|::|\\\\.|->)cancel$\", \"path\": \"app\"}."
    )]
    async fn find_callers(&self, Parameters(p): Parameters<FindCallersParams>) -> CallToolResult {
        self.execute(CommandKind::FindCallers, ShowQuery::find_callers(p))
            .await
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Where a string literal (error message, key) or a constant is used/defined (substring match). Example: {\"kind\": \"strings\", \"pattern\": \"Order not found\"}. Calls of a function: `find_callers`; which files to start a task in: `locate`, not this."
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> CallToolResult {
        self.execute(CommandKind::Search, ShowQuery::search(p))
            .await
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Framework wiring of the directory (or file) holding the definitions: which route → which controller, which event → which listener; also middleware, validation, jobs, providers, magic, di, views; inertia: page ← render site, props; ziggy: JS route('name') calls checked against the Laravel route table; forms: Inertia useForm fields checked against the controller validation rules; models: Eloquent models with their table columns (from migrations), fillable/casts and relations. Example: {\"view\": \"routes\", \"path\": \"routes\"}. Structure of a file or folder: `outline`."
    )]
    async fn framework_view(
        &self,
        Parameters(p): Parameters<FrameworkViewParams>,
    ) -> CallToolResult {
        self.execute(CommandKind::FrameworkView, ShowQuery::framework_view(p))
            .await
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Notes (pilot) on files or methods by people/agents: `path[::method]  time  📝 text`, [orphaned] if the method is gone. Read-only (written with the CLI `callgraph note add`). Example: {\"pattern\": \"refund\", \"path\": \"app\"}; no `pattern`: all notes."
    )]
    async fn notes(&self, Parameters(p): Parameters<NotesParams>) -> CallToolResult {
        self.execute(CommandKind::Notes, ShowQuery::notes(p)).await
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
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
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Project profile from folder and file names, checked against the code graph (~15 lines): architecture (Laravel/Yii MVC, DDD/hexagonal, Vue/React, SwiftUI MVVM, Rust web), layers with folders and file counts, patterns, call direction between layers, unknown folders, notable packages with locked versions (`stack:`); `packages: true` lists every direct composer / npm dependency. Call it first in an unfamiliar project, then pass `layer` (e.g. \"service\") to `outline` / `search` / `find_callers`. Example: {} or {\"full\": true}. Pitfall: a layer name comes from this list (controller, service, repository, model, component, store…), not a folder path."
    )]
    async fn project_profile(&self, Parameters(p): Parameters<ProfileParams>) -> CallToolResult {
        let started = Instant::now();
        let project = p.project.clone();
        let runner = Arc::clone(&self.runner);
        // a statisztikában (és a parancs-gráfban) külön lépés: `deps`
        let kind = if p.deps {
            CommandKind::Deps
        } else {
            CommandKind::ProjectProfile
        };
        // `deps: true`: a profil helyett a függőség-vizsgálat (ritka, igény szerinti)
        let result = tokio::task::spawn_blocking(move || {
            if p.deps {
                runner.deps(p.project.as_deref())
            } else {
                runner.profile(p.project.as_deref(), p.full, p.packages)
            }
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        let call = Self::plain(kind, started, project.as_deref());
        self.finish(&call, result)
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Where and how the code uses a composer / npm dependency: per API symbol the count, first file and +N; `calls: true` lists every call per file. Use before upgrading or replacing a package. Example: {\"package\": \"guzzle\"}."
    )]
    async fn package_usage(&self, Parameters(p): Parameters<PackageUsageParams>) -> CallToolResult {
        let started = Instant::now();
        let project = p.project.clone();
        let runner = Arc::clone(&self.runner);
        let result = tokio::task::spawn_blocking(move || {
            runner.uses(p.project.as_deref(), &p.package, p.calls)
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        let call = Self::plain(CommandKind::PackageUsage, started, project.as_deref());
        self.finish(&call, result)
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Run a suggested next step by number instead of composing the call: some results end with `↪ r7 possible next: [1] … [2] …` (server run with `--next`). Use it only if a suggestion fits your task; else ignore the line. Example: {\"result_id\": \"r7\", \"choice\": [1, 3]}. Ids are per project and expire after 24 hours."
    )]
    async fn next(&self, Parameters(p): Parameters<NextParams>) -> CallToolResult {
        let started = Instant::now();
        let project = p.project.clone();
        let runner = Arc::clone(&self.runner);
        let result = tokio::task::spawn_blocking(move || {
            let choices = crate::next::parse_choices(&p.choice.to_text()).ok_or_else(|| {
                crate::tf!(
                    "Not a choice: {} (expected 1, 2, 3 or a list like [1, 3]).",
                    "Nem választás: {} (1, 2, 3 vagy lista, pl. [1, 3]).",
                    p.choice.to_text()
                )
            })?;
            if crate::next::parse_id(&p.result_id).is_none() {
                return Err(super::runner::unknown_result(&p.result_id));
            }
            runner.next(p.project.as_deref(), &p.result_id, &choices)
        })
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
        let call = Self::plain(CommandKind::Next, started, project.as_deref());
        self.finish(&call, result)
    }

    #[tool(
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        ),
        description = "Generate PHP or Vue from a short expression and write it (no overwrite; backup and syntax check first). E.g. {\"expr\": \"c!:Pay<Base> > ctor(-Client $http)\", \"write\": \"app/Pay.php\"}. Also vue: comp: laravel:Fee>create(name:str); into, replace, dry."
    )]
    async fn generate(&self, Parameters(p): Parameters<GenerateParams>) -> CallToolResult {
        let started = Instant::now();
        let project = p.project.clone();
        let runner = Arc::clone(&self.runner);
        let result = tokio::task::spawn_blocking(move || runner.generate(&p))
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        let call = Self::plain(CommandKind::Generate, started, project.as_deref());
        self.finish(&call, result)
    }

    #[tool(
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "The full callgraph manual (SKILL.md in the configured language): every query with CLI examples, language specifics, formats, limits. Call it once when the server instructions are not enough; CLI flags map to these tools (see its MCP section)."
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
            .with_server_info(Implementation::new(
                "callgraph",
                crate::update::build_version(),
            ))
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
