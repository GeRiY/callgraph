//! Az MCP-mérés: a szerver minden eszköz-hívás végén egyszer hívja (`McpStats::tool_call`). Itt él a munkamenet
//! `locate`-követése is (`LocateHits`, csak memóriában).

use std::sync::Mutex;
use std::time::Instant;

use super::Recorder;
use super::buckets::{LatencyBucket, SizeBucket};
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome, truncated};
use super::locate_hits::{LocateHits, ranked_files};
use crate::skill::SkillHome;

/// Egy befejezett eszköz-hívás adatai. A `project` és az `opened` csak a memóriában élő `locate`-követéshez kell,
/// nem tárolódik.
pub(crate) struct ToolCall<'a> {
    pub(crate) tool: CommandKind,
    pub(crate) started: Instant,
    /// A projekt-alias (`None` = a szerver munkakönyvtára).
    pub(crate) project: Option<&'a str>,
    /// A hívás célja (fájl vagy mappa), ha van.
    pub(crate) path: Option<&'a str>,
    /// A paraméterek már a lekérdezés előtt hibásak voltak.
    pub(crate) invalid: bool,
}

/// Egy MCP-munkamenet mérője.
pub(crate) struct McpStats {
    recorder: Recorder,
    hits: Mutex<LocateHits>,
}

impl McpStats {
    /// A folyamat környezetéből.
    pub(crate) fn from_env(home: &SkillHome) -> Self {
        Self::new(Recorder::from_env(home))
    }

    pub(crate) fn new(recorder: Recorder) -> Self {
        Self {
            recorder,
            hits: Mutex::new(LocateHits::default()),
        }
    }

    /// Egy eszköz-hívás vége: a hívás rögzítése, és a `locate`-követés léptetése (ha most zárult egy lista, a
    /// találat sávja is rögzül). Kikapcsolt mérésnél semmi.
    pub(crate) fn tool_call(&self, call: &ToolCall<'_>, result: &Result<String, String>) {
        if !self.recorder.enabled() {
            return;
        }
        let (outcome, chars, cut) = match result {
            Ok(text) => (Outcome::Ok, text.chars().count(), truncated(text)),
            Err(_) if call.invalid => (Outcome::Error(ErrorKind::Usage), 0, false),
            Err(message) => (
                Outcome::Error(ErrorKind::of_message(message)),
                message.chars().count(),
                false,
            ),
        };
        self.recorder.record(&Event {
            command: call.tool,
            client: Client::Mcp,
            outcome,
            latency: LatencyBucket::from_millis(call.started.elapsed().as_millis()),
            size: SizeBucket::from_chars(chars),
            truncated: cut,
        });
        let hit = {
            let mut hits = self.hits.lock().unwrap_or_else(|e| e.into_inner());
            match (call.tool, result) {
                (CommandKind::Locate, Ok(text)) => hits.listed(call.project, ranked_files(text)),
                (CommandKind::Outline | CommandKind::MethodBody | CommandKind::FindCallers, _) => {
                    hits.followed(call.project, call.path.filter(|p| !p.is_empty()))
                }
                _ => hits.followed(call.project, None),
            }
        };
        if let Some(hit) = hit {
            self.recorder.record_hit(hit);
        }
    }
}
