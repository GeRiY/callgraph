//! MCP-szerver mód (`callgraph mcp`): Model Context Protocol stdio-n, a hivatalos Rust SDK-val (`rmcp`).
//! Csak olvas, hálózatot nem használ; a lekérdezéseket a meglévő `show` parancs futtatja, így a kimenet és a
//! kimeneti korlát azonos a CLI-ével. Részletek: `docs/MCP.md`.

mod guard;
mod params;
mod request;
mod runner;
mod server;

use std::sync::Arc;

use rmcp::ServiceExt;

use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;

/// Elindítja a szervert a folyamat stdin/stdout-ján, és a kliens bontásáig fut.
pub(crate) fn serve_stdio(home: SkillHome) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let server = server::CallgraphServer::new(Arc::new(runner::ShowRunner::new(home, cwd)));
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
