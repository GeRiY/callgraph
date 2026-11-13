//! `callgraph hook session-start` — a Claude Code `SessionStart` hookja (csak új sessionnél, `startup`).
//!
//! Hálózat nélkül, gyorsan megnézi, mi nincs bekapcsolva az ajánlott beállítások közül, és ha talál valamit, egy
//! rövid sort mutat a felhasználónak (`systemMessage`), az agentnek pedig utasítást ad (`additionalContext`):
//! kérdezze meg a felhasználót, és csak „igen” esetén futtassa a megadott parancsot. Ha minden rendben, semmit sem
//! ír ki. Ajánlott tételek:
//! - a fájl-írás utáni háttérfrissítő hook (`hook install`),
//! - az MCP-szerver regisztrálása `--auto-update`-tel (ha van `claude` CLI),
//! - a telemetria (opt-in) — ezt **csak a felhasználó kapcsolhatja be, terminálban** (`telemetry on`, y/N); az agent
//!   nem kapcsolhatja be helyette (a hozzájárulásnak a felhasználótól kell jönnie),
//! - frissítés-értesítés (a meglévő napi háttér-ellenőrzés eredménye, `update::check`).
//!
//! Ritkítás: egy tétel legfeljebb 2 naponta egyszer, összesen legfeljebb 3-szor (a telemetria 10-szer, és csak amíg
//! a döntés nyitott: nem döntött / „most nem”; `Never` / `Off` után soha); a `hook never <tétel>` után soha.
//! Kikapcsolás: `CALLGRAPH_RECOMMEND=0`; `CI` mellett soha. Az állapot: `<skill>/recommend.json`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Egy tétel legfeljebb ennyi naponta jelenik meg.
pub(crate) const INTERVAL_DAYS: i64 = 2;
/// Egy tétel összesen legfeljebb ennyiszer jelenik meg (alapból; tételenként: `Item::max_shown`).
pub(crate) const MAX_SHOWN: u32 = 3;
/// A telemetria-ajánlás összesen legfeljebb ennyiszer jelenik meg.
pub(crate) const MAX_SHOWN_TELEMETRY: u32 = 10;
/// Az állapotfájl neve a skill-mappában.
pub(crate) const STATE_FILE: &str = "recommend.json";

/// Egy ajánlott tétel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Item {
    Hook,
    Mcp,
    Telemetry,
}

impl Item {
    pub(crate) const ALL: [Item; 3] = [Item::Hook, Item::Mcp, Item::Telemetry];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Hook => "hook",
            Self::Mcp => "mcp",
            Self::Telemetry => "telemetry",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.key() == text.trim())
    }

    /// Összesen legfeljebb ennyiszer jelenik meg.
    pub(crate) fn max_shown(self) -> u32 {
        match self {
            Self::Telemetry => MAX_SHOWN_TELEMETRY,
            Self::Hook | Self::Mcp => MAX_SHOWN,
        }
    }
}

/// Az MCP-regisztráció állapota.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpState {
    /// Nincs `claude` CLI (nem tudjuk / nem tudjuk felvenni): nem ajánljuk.
    Unknown,
    Missing,
    WithoutAutoUpdate,
    Ok,
}

/// A gép állapota (a hívó gyűjti össze; I/O nélkül tesztelhető a döntés).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) hook_installed: bool,
    pub(crate) mcp: McpState,
    /// A telemetria-döntés nyitott (még nem döntött, vagy „később”).
    pub(crate) telemetry_open: bool,
    /// Újabb kiadás (a napi ellenőrzés szerint), ha erről most szólni kell.
    pub(crate) update: Option<String>,
}

/// Egy tétel számlálója.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Counter {
    pub(crate) shown: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_day: Option<i64>,
    pub(crate) never: bool,
}

/// Az ajánlások állapota tételenként.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct State {
    pub(crate) items: BTreeMap<String, Counter>,
}

impl State {
    pub(crate) fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string(self).map_err(std::io::Error::other)? + "\n";
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    fn counter(&self, item: Item) -> Counter {
        self.items.get(item.key()).cloned().unwrap_or_default()
    }

    /// A tétel elhallgattatása (a felhasználó nemet mondott).
    pub(crate) fn never(&mut self, item: Item) {
        self.items.entry(item.key().to_owned()).or_default().never = true;
    }

    /// Egy tétel (vagy `None`: mindegyik) számlálójának törlése: újra ajánlható.
    pub(crate) fn reset(&mut self, item: Option<Item>) {
        match item {
            Some(item) => {
                self.items.remove(item.key());
            }
            None => self.items.clear(),
        }
    }
}

/// Szükséges-e a tétel a gép állapota szerint.
fn needed(item: Item, facts: &Facts) -> bool {
    match item {
        Item::Hook => !facts.hook_installed,
        Item::Mcp => matches!(facts.mcp, McpState::Missing | McpState::WithoutAutoUpdate),
        Item::Telemetry => facts.telemetry_open,
    }
}

/// Esedékes-e a tétel a számlálója szerint (nem tiltott, nem fogyott el, és eltelt az intervallum).
fn due(item: Item, counter: &Counter, today: i64) -> bool {
    !counter.never
        && counter.shown < item.max_shown()
        && counter
            .last_day
            .is_none_or(|last| (today - last).abs() >= INTERVAL_DAYS)
}

/// A döntés: a most megjelenítendő tételek és az új állapot (a megjelenítettek számlálója nő).
pub(crate) fn decide(state: &State, facts: &Facts, today: i64) -> (Vec<Item>, State) {
    let mut next = state.clone();
    let mut show = Vec::new();
    for item in Item::ALL {
        if needed(item, facts) && due(item, &state.counter(item), today) {
            let counter = next.items.entry(item.key().to_owned()).or_default();
            counter.shown += 1;
            counter.last_day = Some(today);
            show.push(item);
        }
    }
    (show, next)
}

/// A hook kimenete (JSON), ha van mit mondani; különben `None` (semmit sem ír ki).
pub(crate) fn render(items: &[Item], facts: &Facts, commands: &Commands) -> Option<String> {
    if items.is_empty() && facts.update.is_none() {
        return None;
    }
    let mut user = Vec::new();
    let mut agent = Vec::new();
    if let Some(latest) = &facts.update {
        user.push(crate::tf!(
            "• callgraph {latest} is available (installed: {}) — update: {}",
            "• elérhető a callgraph {latest} (telepítve: {}) — frissítés: {}",
            commands.current,
            commands.update
        ));
        agent.push(format!(
            "- update: callgraph {latest} is available (installed {}). Ask the user whether to update; only if they \
             agree, run `{}`.",
            commands.current, commands.update
        ));
    }
    for item in items {
        match item {
            Item::Hook => {
                user.push(
                    crate::t!(
                        "• Background refresh after edits is off (keeps the project map fresh while you work).",
                        "• A szerkesztés utáni háttérfrissítés ki van kapcsolva (frissen tartja a projekt-térképet munka közben)."
                    )
                    .to_owned(),
                );
                agent.push(format!(
                    "- hook: ask the user whether to turn on callgraph's background refresh after file edits; if yes \
                     run `{}`; if no run `{}`.",
                    commands.hook_install,
                    commands.never(Item::Hook)
                ));
            }
            Item::Mcp => {
                user.push(
                    crate::t!(
                        "• The callgraph MCP server (with automatic updates) is not registered for Claude Code.",
                        "• A callgraph MCP-szerver (automatikus frissítéssel) nincs regisztrálva a Claude Code-ban."
                    )
                    .to_owned(),
                );
                agent.push(format!(
                    "- mcp: ask the user whether to register the callgraph MCP server with automatic updates; if yes \
                     run `{}` (takes effect in the next session); if no run `{}`.",
                    commands.mcp_register,
                    commands.never(Item::Mcp)
                ));
            }
            Item::Telemetry => {
                let telemetry_on = &commands.telemetry_on;
                let preview = &commands.telemetry_preview;
                user.push(crate::tf!(
                    "⭐ Help callgraph get better for your stack: share anonymous usage counts (which commands, in which order, how fast — never code, file names or paths). It directly tunes locate ranking and next-step suggestions for the languages and frameworks you use. One command, off any time: {telemetry_on} · see exactly what is sent: {preview}",
                    "⭐ Segíts, hogy a callgraph a te stackedben legyen jobb: oszd meg a névtelen használati számokat (melyik parancs, milyen sorrendben, milyen gyorsan — kód, fájlnév és útvonal soha). Ezekből közvetlenül a locate rangsorát és a javasolt következő lépéseket hangoljuk a nyelveidre és keretrendszereidre. Egy parancs, bármikor kikapcsolható: {telemetry_on} · pontosan mi menne el: {preview}"
                ));
                agent.push(format!(
                    "- telemetry: do NOT turn it on yourself and do not run `telemetry on`. Just mention that the user \
                     can opt in by running `{}` in their own terminal. If they say they do not want it, run `{}`.",
                    commands.telemetry_on,
                    commands.never(Item::Telemetry)
                ));
            }
        }
    }
    let message = format!(
        "{}\n{}",
        crate::t!("callgraph:", "callgraph:"),
        user.join("\n")
    );
    let context = format!(
        "callgraph session check. For each item below, ask the user once, briefly, at a natural point early in the \
         session (not in the middle of their task), and act only on an explicit yes. Never change these settings \
         without asking.\n{}",
        agent.join("\n")
    );
    let out = serde_json::json!({
        "systemMessage": message,
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context,
        },
    });
    Some(out.to_string())
}

/// A felhasználónak / agentnek mutatott parancsok (a valódi bináris- és skill-utakkal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Commands {
    pub(crate) exe: String,
    pub(crate) current: String,
    pub(crate) update: String,
    pub(crate) hook_install: String,
    pub(crate) mcp_register: String,
    pub(crate) telemetry_on: String,
    /// `<script> telemetry preview`: pontosan mi menne el.
    pub(crate) telemetry_preview: String,
}

impl Commands {
    pub(crate) fn never(&self, item: Item) -> String {
        format!("{} hook never {}", self.exe, item.key())
    }
}

/// Ki van-e kapcsolva az ajánlás (`CALLGRAPH_RECOMMEND=0|off|false|no`, vagy `CI`).
pub(crate) fn disabled(get: impl Fn(&str) -> Option<String>) -> bool {
    let off = get("CALLGRAPH_RECOMMEND").is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        )
    });
    off || get("CI").is_some_and(|v| !v.trim().is_empty())
}

/// Az MCP-regisztráció a `~/.claude.json` (vagy `$CLAUDE_CONFIG_DIR/.claude.json`) user-scope `mcpServers` részéből.
pub(crate) fn mcp_state(claude_json: Option<&str>, claude_cli: bool) -> McpState {
    if !claude_cli {
        return McpState::Unknown;
    }
    let Some(text) = claude_json else {
        return McpState::Missing;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return McpState::Unknown;
    };
    let Some(server) = value.get("mcpServers").and_then(|s| s.get("callgraph")) else {
        return McpState::Missing;
    };
    let args: Vec<&str> = server
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    if args.contains(&"--auto-update") {
        McpState::Ok
    } else {
        McpState::WithoutAutoUpdate
    }
}
