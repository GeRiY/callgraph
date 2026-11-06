//! Claude Code-hook a fájl-írásokra: a `PostToolUse` hook (Edit / Write / MultiEdit) a `callgraph hook post-edit`
//! parancsot futtatja, amely a hook stdin-jén kapott JSON-ból kiveszi a szerkesztett fájlt, megkeresi a projektjét
//! (a legközelebbi regisztrált alias-gyökér, vagy egy már létező `.callgraph/` mappa), és a háttérben elindítja a
//! `callgraph refresh`-t rá — így a következő lekérdezés nem elemez. A `~/.claude/settings.json` szerkesztése
//! (`hook install|uninstall|status`) is itt van: a többi beállítást és hookot megtartja, idempotens.

mod payload;
mod settings;

#[cfg(test)]
mod tests;

pub(crate) use payload::{edited_file, project_root_for};
pub(crate) use settings::{
    HOOK_EVENT, HOOK_MATCHER, Settings, hook_command, install, is_installed, uninstall,
};
