//! MCP-szerver mód (`callgraph mcp`): Model Context Protocol stdio-n, a hivatalos Rust SDK-val (`rmcp`).
//! Alapból csak olvas (az egyetlen író eszköz, a `generate`, opt-in: `--generate` / `CALLGRAPH_GENERATE=1`); hálózatot csak a frissítés használ (`autoupdate`: napi háttér-ellenőrzés, opt-in `--auto-update`),
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
pub(crate) fn serve_stdio(
    home: SkillHome,
    auto_update: bool,
    allow: &[String],
    err: &mut dyn std::io::Write,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let allowed = allowed_roots(&cwd, allow, err);
    // a betöltött projekt-gyökerek figyelése: a változott fájlok a háttérben frissülnek (`graph::watch`)
    crate::graph::watch::enable();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut runner: Arc<dyn runner::QueryRunner> =
            Arc::new(runner::ShowRunner::new(home.clone(), cwd).with_allowed(allowed));
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

/// Az MCP-őr által a projekt-gyökéren kívül is elfogadott gyökerek: a `--allow` mappák és a `CALLGRAPH_MCP_ALLOW`
/// környezeti változó (az OS útvonal-elválasztójával, Unixon `:`), a munkakönyvtárhoz képest feloldva, kanonikusan.
/// A nem létező / nem mappa elem figyelmeztetéssel kimarad.
fn allowed_roots(
    cwd: &std::path::Path,
    allow: &[String],
    err: &mut dyn std::io::Write,
) -> Vec<std::path::PathBuf> {
    let from_env = std::env::var_os(ALLOW_ENV)
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for raw in allow.iter().map(std::path::PathBuf::from).chain(from_env) {
        if raw.as_os_str().is_empty() {
            continue;
        }
        match cwd.join(&raw).canonicalize() {
            Ok(dir) if dir.is_dir() => {
                if !out.contains(&dir) {
                    out.push(dir);
                }
            }
            _ => {
                let _ = writeln!(
                    err,
                    "{}",
                    crate::tf!(
                        "Ignoring --allow {}: not a directory",
                        "A --allow {} figyelmen kívül marad: nem mappa",
                        raw.display()
                    )
                );
            }
        }
    }
    out
}

/// Az engedélyezett extra gyökerek környezeti változója (a `--allow` párja, MCP-konfigurációba).
pub(crate) const ALLOW_ENV: &str = "CALLGRAPH_MCP_ALLOW";

fn failure(detail: String) -> CallgraphError {
    CallgraphError::Usage(crate::tf!(
        "MCP server error: {detail}",
        "MCP-szerver hiba: {detail}"
    ))
}
