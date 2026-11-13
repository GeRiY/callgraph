//! A CLI-mérés: a `main.rs` egyetlen helyen indítja (`CliProbe::start`) és zárja (`finish`); a kimenetet egy
//! számláló író (`Meter`) adja tovább, amely csak a karakterszámot és a kimenet első legfeljebb 64 KiB-ját tartja
//! meg (az eleje a levágás-figyelmeztetéshez, az egész minta az előző kimenet elemeihez, `reuse.rs`; csak a
//! memóriában). A háttérben indított gyerekfolyamatok (`CLIENT_ENV`) a hook frissítései kivételével nem mérnek; a
//! `callgraph mcp` és a `callgraph stats` sem (az MCP-hívásokat a szerver méri).

use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

use super::Recorder;
use super::buckets::{LatencyBucket, SizeBucket};
use super::event::{Client, CommandKind, ErrorKind, Event, Outcome, truncated};
use super::events::now_ms;
use super::reuse::{OutputItems, SAMPLE_BYTES};
use super::target::{Call, cli_symbols, cli_target};
use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;

/// A leválasztott gyerekfolyamat jelzője (`update::check::spawn_detached_as`): `hook` = a hook háttér-frissítése
/// (mérjük, hook-kliensként), bármi más (`background`) = nem mérjük (a szülő már mért).
pub(crate) const CLIENT_ENV: &str = "CALLGRAPH_STATS_CLIENT";

/// A futás közben jelzett hiba-kategória (`note_failure`): a skill-parancs (`wrap`, `project`…) így adja át, hogy
/// felhasználói vagy belső hiba volt-e (a kilépési kód mindkettőnél 1).
static NOTED: Mutex<Option<ErrorKind>> = Mutex::new(None);

/// Egy skill-hiba kontextusának jelzése a CLI-mérésnek (`skill::finish`).
pub(crate) fn note_failure(context: &str) {
    *NOTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(ErrorKind::of_context(context));
}

fn noted_failure() -> Option<ErrorKind> {
    NOTED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// Ennyi bájtot tart meg a kimenet elejéből a levágás-figyelmeztetés felismeréséhez.
const HEAD_BYTES: usize = 512;

/// Egy CLI-futás mérése.
pub struct CliProbe {
    target: Option<(CommandKind, Client)>,
    started: Instant,
    recorder: Recorder,
    /// A nyers argumentumok (csak a hívás céljának megkereséséhez a végén, `target::cli_target`; nem tárolódik).
    args: Vec<String>,
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
            args: if recorder.enabled() {
                args.to_vec()
            } else {
                Vec::new()
            },
            recorder,
        }
    }

    /// Ritka sor az agentnek a beküldés lehetőségéről (`telemetry::Telemetry::agent_hint`): csak sikeres, mért
    /// parancssori lekérdezés (`show` / `locate` / `profile`) végén.
    pub fn agent_hint(&self) -> Option<String> {
        let (command, Client::Cli) = self.target? else {
            return None;
        };
        if !self.recorder.enabled()
            || !matches!(
                command,
                CommandKind::Show | CommandKind::Locate | CommandKind::Profile
            )
        {
            return None;
        }
        super::telemetry::Telemetry::from_env(&SkillHome::from_env())
            .agent_hint(super::telemetry::now_secs())
    }

    /// Az argumentumok hibásak voltak (a parancs el sem indult).
    pub fn failed(self, error: &CallgraphError) {
        self.record(Outcome::Error(ErrorKind::of(error)), 0, false, None);
    }

    /// A futás vége: az eredmény, a kimenet mérete, és nem nulla kilépési kódnál a hibacsatorna eleje (csak a
    /// kategóriához: „nincs meg” / használat / egyéb; a szöveg nem tárolódik).
    pub fn finish<W: Write, E: Write>(self, result: &Result<i32>, out: &Meter<W>, err: &Meter<E>) {
        let outcome = match result {
            // `--strict`: a 3-as kód (szintaktikai hibás fájl) sikeres lekérdezés
            Ok(0 | crate::cli::strict::STRICT_EXIT_CODE) => Outcome::Ok,
            // a skill-parancs pontos kategóriája (`note_failure`), különben a 2-es kód használati hiba
            Ok(code) => Outcome::Error(noted_failure().unwrap_or_else(|| {
                if *code == 2 {
                    ErrorKind::Usage
                } else {
                    ErrorKind::of_message(&String::from_utf8_lossy(err.head()))
                }
            })),
            Err(error) => Outcome::Error(ErrorKind::of(error)),
        };
        let head = String::from_utf8_lossy(out.head());
        let cut = truncated(&head);
        // az előző-kimenet összevetéshez csak sikeres futás mintája kell (a hibás futás után a következő `-`)
        let output = match outcome {
            Outcome::Ok if self.recorder.enabled() => {
                OutputItems::from_text(&String::from_utf8_lossy(out.sample()))
            }
            _ => None,
        };
        self.record(outcome, out.chars(), cut, output);
    }

    fn record(self, outcome: Outcome, chars: usize, truncated: bool, output: Option<OutputItems>) {
        let Some((command, client)) = self.target else {
            return;
        };
        if !self.recorder.enabled() {
            return;
        }
        let telemetry = super::telemetry::Telemetry::from_env(&SkillHome::from_env());
        // egyszer gépenként, csak a parancssorban (a hook háttérfolyamatának nincs terminálja)
        if client == Client::Cli {
            if let Some(notice) = self.recorder.first_notice() {
                eprintln!("{notice}");
            }
            // a már bekapcsolt telemetriánál egyszer: mostantól a parancs-sorrend számai is mennek
            if let Some(notice) = telemetry.order_notice() {
                eprintln!("{notice}");
            }
        }
        let elapsed = self.started.elapsed().as_millis();
        let cwd = std::env::current_dir().unwrap_or_default();
        self.recorder.record_call(&Call {
            event: Event {
                command,
                client,
                outcome,
                latency: LatencyBucket::from_millis(elapsed),
                size: SizeBucket::from_chars(chars),
                truncated,
            },
            end_ms: now_ms(),
            ms: u64::try_from(elapsed).unwrap_or(u64::MAX),
            target: cli_target(command, &self.args, &cwd),
            symbols: cli_symbols(command, &self.args),
            output,
        });
        // opt-in beküldés a normál futásokra ráülve (bekapcsolva és esedékesen: leválasztott háttérfolyamat)
        telemetry.maybe_spawn(super::telemetry::now_secs());
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
        // a `deps` ritka, a profil része (az MCP-ben `project_profile` `deps: true`): nincs külön fajtája
        "profile" | "deps" => CommandKind::Profile,
        "refresh" => CommandKind::Refresh,
        "callers" => CommandKind::Callers,
        "note" => CommandKind::Notes,
        "project" => CommandKind::Projects,
        "next" => CommandKind::Next,
        "uses" => CommandKind::PackageUsage,
        _ => CommandKind::Other,
    };
    Some((command, client))
}

/// Számláló író: továbbad, és megjegyzi a karakterszámot (UTF-8) meg az első `SAMPLE_BYTES` bájtot (ennek eleje a
/// `head`).
pub struct Meter<W> {
    inner: W,
    chars: usize,
    sample: Vec<u8>,
}

impl<W: Write> Meter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            chars: 0,
            sample: Vec::new(),
        }
    }

    /// Az eddig átment karakterek száma.
    pub(crate) fn chars(&self) -> usize {
        self.chars
    }

    /// A kimenet első (legfeljebb `HEAD_BYTES`) bájtja.
    pub(crate) fn head(&self) -> &[u8] {
        &self.sample[..self.sample.len().min(HEAD_BYTES)]
    }

    /// A kimenet első (legfeljebb `SAMPLE_BYTES`) bájtja.
    pub(crate) fn sample(&self) -> &[u8] {
        &self.sample
    }
}

impl<W: Write> Write for Meter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        let written = &buf[..n];
        // a UTF-8 folytató bájtok (10xxxxxx) nem új karakterek
        self.chars += written.iter().filter(|b| (**b & 0xC0) != 0x80).count();
        let room = SAMPLE_BYTES.saturating_sub(self.sample.len());
        self.sample
            .extend_from_slice(&written[..room.min(written.len())]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
