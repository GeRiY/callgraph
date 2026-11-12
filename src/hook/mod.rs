//! Claude Code-hookok:
//! - a `PostToolUse` hook (Edit / Write / MultiEdit) a `callgraph hook post-edit` parancsot futtatja, amely a hook
//!   stdin-jén kapott JSON-ból kiveszi a szerkesztett fájlt, megkeresi a projektjét (a legközelebbi regisztrált
//!   alias-gyökér, vagy egy már létező `.callgraph/` mappa), és a háttérben elindítja a `callgraph refresh`-t rá —
//!   így a következő lekérdezés nem elemez;
//! - a `SessionStart` hook (`callgraph hook session-start`): az ajánlott, de kikapcsolt beállítások és a
//!   frissítés-értesítés a session elején (`session_start`).
//!
//! A `~/.claude/settings.json` szerkesztése (`hook install|install-session|uninstall|status`) is itt van: a többi
//! beállítást és hookot megtartja, idempotens.

mod payload;
pub(crate) mod session_start;
mod settings;

#[cfg(test)]
mod session_start_tests;
#[cfg(test)]
mod tests;

pub(crate) use payload::{edited_file, project_root_for};
#[cfg(test)]
pub(crate) use settings::uninstall;
pub(crate) use settings::{
    HOOK_EVENT, HOOK_MATCHER, POST_EDIT, SESSION_START, Settings, hook_command, hook_command_for,
    install, install_for, is_installed, is_installed_for, uninstall_for,
};
