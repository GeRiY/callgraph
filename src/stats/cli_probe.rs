//! A CLI-mérés: a `main.rs` egyetlen helyen indítja (`CliProbe::start`) és zárja (`finish`); a kimenetet egy
//! számláló író (`Meter`) adja tovább, amely csak a karakterszámot és az első néhány száz bájtot (a levágás-
//! figyelmeztetéshez) tartja meg. A háttérben indított gyerekfolyamatok (`CLIENT_ENV`) a hook frissítései
//! kivételével nem mérnek; a `callgraph mcp` és a `callgraph stats` sem (az MCP-hívásokat a szerver méri).

use std::io::Write;
use std::time::Instant;

use super::Recorder;
use super::buckets::{LatencyBucket, SizeBucket};
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome, truncated};
use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;

/// A leválasztott gyerekfolyamat jelzője (`update::check::spawn_detached_as`): `hook` = a hook háttér-frissítése
/// (mérjük, hook-kliensként), bármi más (`background`) = nem mérjük (a szülő már mért).
pub(crate) const CLIENT_ENV: &str = "CALLGRAPH_STATS_CLIENT";

/// Ennyi bájtot tart meg a kimenet elejéből a levágás-figyelmeztetés felismeréséhez.
const HEAD_BYTES: usize = 512;

/// Egy CLI-futás mérése.
pub struct CliProbe {
    target: Option<(CommandKind, Client)>,
    started: Instant,
    recorder: Recorder,
}

impl CliProbe {
    /// A futás elején, a nyers argumentumokból (a program neve nélkül).
    pub fn start(args: &[String]) -> Self {
        let started = Instant::now();
        let target = classify(args, std::env::var(CLIENT_ENV).ok().as_deref());
        let recorder = match target {
            Some(_) => Recorder::from_env(&SkillHome::from_env()),
            None => Recorder::disabled(),
        };
        Self {
            target,
            started,
            recorder,
        }
    }

    /// Az argumentumok hibásak voltak (a parancs el sem indult).
    pub fn failed(self, error: &CallgraphError) {
        self.record(Outcome::Error(ErrorKind::of(error)), 0, false);
    }

    /// A futás vége: az eredmény, a kimenet mérete, és nem nulla kilépési kódnál a hibacsatorna eleje (csak a
    /// kategóriához: „nincs meg” / használat / egyéb; a szöveg nem tárolódik).
    pub fn finish<W: Write, E: Write>(self, result: &Result<i32>, out: &Meter<W>, err: &Meter<E>) {
        let outcome = match result {
            // `--strict`: a 3-as kód (szintaktikai hibás fájl) sikeres lekérdezés
            Ok(0 | crate::cli::strict::STRICT_EXIT_CODE) => Outcome::Ok,
            Ok(_) => Outcome::Error(ErrorKind::of_message(&String::from_utf8_lossy(err.head()))),
            Err(error) => Outcome::Error(ErrorKind::of(error)),
        };
        let head = String::from_utf8_lossy(out.head());
        self.record(outcome, out.chars(), truncated(&head));
    }

    fn record(self, outcome: Outcome, chars: usize, truncated: bool) {
        let Some((command, client)) = self.target else {
            return;
        };
        if !self.recorder.enabled() {
            return;
        }
        self.recorder.record(&Event {
            command,
            client,
            outcome,
            latency: LatencyBucket::from_millis(self.started.elapsed().as_millis()),
            size: SizeBucket::from_chars(chars),
            truncated,
        });
    }
}

/// A parancs és a kliens a nyers argumentumokból; `None` = nem mérjük.
pub(crate) fn classify(args: &[String], client: Option<&str>) -> Option<(CommandKind, Client)> {
    let client = match client {
        None | Some("") => Client::Cli,
        Some("hook") => Client::Hook,
        Some(_) => return None,
    };
    let command = match args.first().map_or("", String::as_str) {
        "mcp" | "stats" => return None,
        "hook" if args.get(1).is_some_and(|a| a == "post-edit") => return None,
        "show" | "wrap" => CommandKind::Show,
        "locate" => CommandKind::Locate,
        "profile" => CommandKind::Profile,
        "refresh" => CommandKind::Refresh,
        "callers" => CommandKind::Callers,
        "note" => CommandKind::Notes,
        "project" => CommandKind::Projects,
        _ => CommandKind::Other,
    };
    Some((command, client))
}

/// Számláló író: továbbad, és megjegyzi a karakterszámot (UTF-8) meg az első `HEAD_BYTES` bájtot.
pub struct Meter<W> {
    inner: W,
    chars: usize,
    head: Vec<u8>,
}

impl<W: Write> Meter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            chars: 0,
            head: Vec::new(),
        }
    }

    /// Az eddig átment karakterek száma.
    pub(crate) fn chars(&self) -> usize {
        self.chars
    }

    /// A kimenet első (legfeljebb `HEAD_BYTES`) bájtja.
    pub(crate) fn head(&self) -> &[u8] {
        &self.head
    }
}

impl<W: Write> Write for Meter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        let written = &buf[..n];
        // a UTF-8 folytató bájtok (10xxxxxx) nem új karakterek
        self.chars += written.iter().filter(|b| (**b & 0xC0) != 0x80).count();
        let room = HEAD_BYTES.saturating_sub(self.head.len());
        self.head
            .extend_from_slice(&written[..room.min(written.len())]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
