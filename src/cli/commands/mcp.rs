//! `callgraph mcp` — MCP-szerver stdio-n (a CLI mellett egy további használati mód; a CLI változatlan).

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::skill::SkillHome;

/// Az MCP-szervert indító parancs; a stdout a protokollé, ezért a kapott írókat nem használja.
pub struct McpCommand {
    home: SkillHome,
}

impl McpCommand {
    pub fn new(home: SkillHome) -> Self {
        Self { home }
    }
}

impl Command for McpCommand {
    fn run(&self, _out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        crate::mcp::serve_stdio(self.home.clone())?;
        Ok(0)
    }
}
