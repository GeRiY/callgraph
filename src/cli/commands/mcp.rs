//! `callgraph mcp [--auto-update]` — MCP-szerver stdio-n (a CLI mellett egy további használati mód; a CLI
//! változatlan). `--auto-update`: indításkor, a kézfogás előtt naponta egyszer frissít (`mcp::autoupdate`).

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::SkillHome;

/// Az MCP-szervert indító parancs; a stdout a protokollé, ezért a kapott írókat nem használja.
pub struct McpCommand {
    home: SkillHome,
    auto_update: bool,
}

impl McpCommand {
    pub fn new(home: SkillHome, auto_update: bool) -> Self {
        Self { home, auto_update }
    }
}

impl Command for McpCommand {
    fn run(&self, _out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        if self.auto_update {
            // sikeres frissítésnél nem tér vissza: az új binárissal indul újra (a stdout-ra itt sem ír)
            crate::mcp::auto_update(&self.home, err);
        }
        crate::mcp::serve_stdio(self.home.clone(), self.auto_update)?;
        Ok(0)
    }
}
