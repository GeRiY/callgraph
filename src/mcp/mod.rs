//! MCP-szerver mód (`callgraph mcp`): Model Context Protocol stdio-n, a hivatalos Rust SDK-val (`rmcp`).
//! Csak olvas; hálózatot csak a frissítés használ (`autoupdate`: napi háttér-ellenőrzés, opt-in `--auto-update`),
//! a skill-mappán belül. A lekérdezéseket a meglévő `show` parancs futtatja, így a kimenet és a
//! kimeneti korlát azonos a CLI-ével. Részletek: `docs/MCP.md`.

mod autoupdate;
mod guard;
mod guide;
mod hint;
mod params;
mod request;
mod runner;
mod server;

use std::sync::Arc;

use rmcp::ServiceExt;

use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;

pub(crate) use autoupdate::{AUTO_UPDATE_FLAG, auto_update};

/// Elindítja a szervert a folyamat stdin/stdout-ján, és a kliens bontásáig fut. `auto_update` nélkül a napi
/// háttér-ellenőrzés az első eszköz-eredmény végén szól, ha újabb verzió van (`autoupdate`).
pub(crate) fn serve_stdio(home: SkillHome, auto_update: bool) -> Result<()> {
    let cwd = std::env::current_dir()?;
    // a betöltött projekt-gyökerek figyelése: a változott fájlok a háttérben frissülnek (`graph::watch`)
    crate::graph::watch::enable();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut runner: Arc<dyn runner::QueryRunner> =
            Arc::new(runner::ShowRunner::new(home.clone(), cwd));
        if !auto_update {
            runner = autoupdate::with_notice(runner, &home);
        }
        // a beküldés-tipp (ritka, a felhasználónak) és az opt-in beküldés óránkénti vizsgálata
        runner = hint::with_hint(runner, &home);
        let stats = crate::stats::McpStats::from_env(&home);
        let usage = crate::update::usage::UsageCheck::from_env(
            &home,
            Box::new(crate::update::usage::ThreadSender),
        );
        let server = server::CallgraphServer::new(runner, stats).with_usage(usage);
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| failure(e.to_string()))?;
        service
            .waiting()
            .await
            .map_err(|e| failure(e.to_string()))?;
        Ok(())
    })
}

fn failure(detail: String) -> CallgraphError {
    CallgraphError::Usage(crate::tf!(
        "MCP server error: {detail}",
        "MCP-szerver hiba: {detail}"
    ))
}
