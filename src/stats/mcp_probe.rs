//! Az MCP-mérés: a szerver minden eszköz-hívás végén egyszer hívja (`McpStats::tool_call`). Itt él a munkamenet
//! `locate`-követése is (`LocateHits`, csak memóriában). A hívás célja (projekt + út) csak a memóriában él: a
//! munkamenet-állapotba kulcsolt hash-ként, a napi fájlba a nyelve és a méret-sávja kerül (`target.rs`).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use super::Recorder;
use super::buckets::{LatencyBucket, SizeBucket};
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome, truncated};
use super::events::now_ms;
use super::locate_hits::{LocateHits, ranked_files};
use super::target::{Call, Target};
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

/// A projekt-gyökér feloldása a cél megkereséséhez (DI): alias (`None` = a szerver munkakönyvtára) → gyökér.
pub(crate) type RootResolver = Box<dyn Fn(Option<&str>) -> Option<PathBuf> + Send + Sync>;

/// Egy MCP-munkamenet mérője.
pub(crate) struct McpStats {
    recorder: Recorder,
    hits: Mutex<LocateHits>,
    roots: RootResolver,
}

impl McpStats {
    /// A folyamat környezetéből: a gyökér az alias registryből (`projects.json`), alias nélkül a munkakönyvtár.
    pub(crate) fn from_env(home: &SkillHome) -> Self {
        let registry = home.registry_file().to_path_buf();
        let cwd = std::env::current_dir().ok();
        Self::new(Recorder::from_env(home)).with_roots(Box::new(move |alias: Option<&str>| {
            match alias.filter(|a| !a.is_empty()) {
                None => cwd.clone(),
                Some(alias) => crate::skill::ProjectRegistry::load(&registry)
                    .ok()?
                    .root(alias)
                    .map(PathBuf::from),
            }
        }))
    }

    /// Gyökér-feloldás nélkül (a cél a kérés szövegéből, `Target::from_text`).
    pub(crate) fn new(recorder: Recorder) -> Self {
        Self {
            recorder,
            hits: Mutex::new(LocateHits::default()),
            roots: Box::new(|_: Option<&str>| None),
        }
    }

    pub(crate) fn with_roots(mut self, roots: RootResolver) -> Self {
        self.roots = roots;
        self
    }

    /// A hívás célja: a feloldott gyökérhez képest a fájlrendszerből, különben a kérés szövegéből.
    fn target(&self, call: &ToolCall<'_>) -> Option<Target> {
        let path = call.path.filter(|p| !p.is_empty())?;
        let resolved = (self.roots)(call.project).and_then(|root| {
            let root = root.canonicalize().ok()?;
            Target::from_path(&root.join(path), Some(root.display().to_string()))
        });
        resolved.or_else(|| Target::from_text(call.project, path))
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
        let elapsed = call.started.elapsed().as_millis();
        self.recorder.record_call(&Call {
            event: Event {
                command: call.tool,
                client: Client::Mcp,
                outcome,
                latency: LatencyBucket::from_millis(elapsed),
                size: SizeBucket::from_chars(chars),
                truncated: cut,
            },
            end_ms: now_ms(),
            ms: u64::try_from(elapsed).unwrap_or(u64::MAX),
            target: self.target(call),
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
