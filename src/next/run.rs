//! A javaslat-sor előállítása (`annotate`: szabályok → tároló → napló → statisztika → a sor), a CLI-parancsok
//! becsomagolása (`decorate`), és egy tárolt javaslat futtatása (`run_choices`: ugyanaz a bináris, folyamaton
//! belül, a tárolt argumentumokkal — a kimenet olyan, mintha a felhasználó futtatta volna).

use std::io::Write;
use std::path::Path;

use super::invocation::Invocation;
use super::log::{EventLog, LogEvent, hash, hash_args};
use super::rules;
use super::store::ResultStore;
use super::suggestion::Suggestion;
use crate::cli::{CliArgs, Command, build_command};
use crate::error::{CallgraphError, Result};
use crate::skill::SkillHome;

/// A CLI-parancs becsomagolása, ha a javaslatok be vannak kapcsolva és a parancshoz van szabály; különben
/// változatlan.
pub fn decorate(args: &[String], command: Box<dyn Command>) -> Box<dyn Command> {
    if !super::enabled() {
        return command;
    }
    match Invocation::parse(args) {
        Some(invocation) => {
            let project = invocation.project().map(str::to_owned);
            Box::new(Suggesting {
                inner: command,
                args: args.to_vec(),
                invocation,
                project,
                observe: true,
            })
        }
        None => command,
    }
}

/// Egy parancs, amelynek sikeres kimenete végére a javaslat-sor kerülhet.
struct Suggesting {
    inner: Box<dyn Command>,
    args: Vec<String>,
    invocation: Invocation,
    /// A sorban visszaadott `--project` (az eredeti hívásé).
    project: Option<String>,
    /// Számít-e lépésnek a visszalépés-jelhez (a `next` által futtatott parancs nem).
    observe: bool,
}

impl Command for Suggesting {
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let mut buffer = Vec::new();
        let result = self.inner.run(&mut buffer, err);
        out.write_all(&buffer)?;
        let code = result?;
        if code != 0 {
            return Ok(code);
        }
        let home = SkillHome::from_env();
        let Ok(root) =
            crate::cli::commands::project_root(&home, self.invocation.project())
        else {
            return Ok(code);
        };
        if self.observe {
            EventLog::new(ResultStore::for_root(&root).dir()).observe(
                &self.invocation.subjects(),
                &self.args,
                now(),
            );
        }
        let text = String::from_utf8_lossy(&buffer);
        if let Some(line) = annotate(&self.args, &root, &text, self.project.as_deref()) {
            writeln!(out, "{line}")?;
        }
        Ok(code)
    }
}

/// A javaslat-sor egy sikeres kimenethez (a javaslatok mentésével, a `shown` naplózásával és a statisztikával);
/// `None`, ha nincs egyértelmű jelölt, vagy a parancshoz nincs szabály. `project`: a sorban visszaadott
/// `--project` érték.
pub(crate) fn annotate(
    args: &[String],
    root: &Path,
    output: &str,
    project: Option<&str>,
) -> Option<String> {
    let invocation = Invocation::parse(args)?;
    let classifier = crate::profile::Classifier::for_root(root);
    let layer_of = |rel: &str| {
        classifier
            .classify(rel)
            .and_then(|c| c.layer())
            .map(|layer| format!("{layer:?}"))
    };
    let suggestions = rules::suggest(&invocation, root, output, &layer_of);
    if suggestions.is_empty() {
        return None;
    }
    let store = ResultStore::for_root(root);
    let kind = invocation.command_kind();
    let t = now();
    let id = store.save(kind.name(), project, &suggestions, t)?;
    EventLog::new(store.dir()).append(&LogEvent::Shown {
        t,
        id,
        cmd: kind.name().to_owned(),
        kinds: suggestions.iter().map(|s| s.kind).collect(),
    });
    crate::stats::Recorder::from_env(&SkillHome::from_env()).record_next(kind, false);
    Some(line(id, &suggestions, project))
}

/// A sor: `↪ r7 possible next: [1] …  [2] …` (`--project` mellett a `next` alakjával).
pub(crate) fn line(id: u64, suggestions: &[Suggestion], project: Option<&str>) -> String {
    let items = suggestions
        .iter()
        .enumerate()
        .map(|(at, s)| format!("[{}] {}", at + 1, s.label))
        .collect::<Vec<_>>()
        .join("  ");
    let mut text = crate::tf!(
        "↪ r{id} possible next: {items}",
        "↪ r{id} lehetséges következő: {items}"
    );
    if let Some(project) = project {
        text.push_str(&format!("  (next r{id} <n> --project {project})"));
    }
    text
}

/// `r7` → 7.
pub(crate) fn parse_id(text: &str) -> Option<u64> {
    let digits = text.trim().strip_prefix('r')?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|n| *n > 0)
}

/// `1` / `1,3` → a választások (1–3, sorrendben, ismétlés nélkül); `None`, ha hibás.
pub(crate) fn parse_choices(text: &str) -> Option<Vec<usize>> {
    let mut choices = Vec::new();
    for part in text.split(',') {
        let n: usize = part.trim().parse().ok()?;
        if !(1..=rules::MAX_SUGGESTIONS).contains(&n) {
            return None;
        }
        if !choices.contains(&n) {
            choices.push(n);
        }
    }
    (!choices.is_empty()).then_some(choices)
}

/// Egy tárolt eredmény választott javaslatainak futtatása sorban (a kimenetek között üres sor). Az utolsó
/// futtatás maga is kaphat javaslat-sort. Ismeretlen / lejárt azonosító, hibás választás: használati hiba.
pub(crate) fn run_choices(
    root: &Path,
    id_text: &str,
    choices: &[usize],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    let store = ResultStore::for_root(root);
    let record = parse_id(id_text)
        .and_then(|id| store.get(id, now()))
        .ok_or_else(|| {
            CallgraphError::Usage(crate::tf!(
                "Unknown or expired result id: {id_text} (ids are per project, kept for 24 hours, the last 50; add --project if the result came from another project) — run the query again.",
                "Ismeretlen vagy lejárt eredmény-azonosító: {id_text} (projektenként, 24 óráig, az utolsó 50 marad meg; más projekt eredményéhez --project kell) — futtasd újra a lekérdezést.",
            ))
        })?;
    let picked: Vec<(usize, &Suggestion)> = choices
        .iter()
        .map(|&n| record.s.get(n - 1).map(|s| (n, s)))
        .collect::<Option<_>>()
        .ok_or_else(|| {
            CallgraphError::Usage(crate::tf!(
                "{id_text} has {} suggestion(s): choose 1–{}.",
                "A(z) {id_text} eredménynek {} javaslata van: 1–{} közül válassz.",
                record.s.len(),
                record.s.len()
            ))
        })?;
    let log = EventLog::new(store.dir());
    let recorder = crate::stats::Recorder::from_env(&SkillHome::from_env());
    let kind = command_kind_named(&record.cmd);
    let last = picked.len().saturating_sub(1);
    for (at, (choice, suggestion)) in picked.into_iter().enumerate() {
        let subjects = Invocation::parse(&suggestion.args)
            .map(|i| i.subjects())
            .unwrap_or_default();
        log.append(&LogEvent::Accepted {
            t: now(),
            id: record.id,
            choice,
            kind: suggestion.kind,
            subj: subjects.iter().map(|s| hash(s)).collect(),
            sig: hash_args(&suggestion.args),
        });
        recorder.record_next(kind, true);
        let mut command = build_command(CliArgs::parse(suggestion.args.clone())?)?;
        if at == last
            && super::enabled()
            && let Some(invocation) = Invocation::parse(&suggestion.args)
        {
            command = Box::new(Suggesting {
                inner: command,
                args: suggestion.args.clone(),
                invocation,
                project: record.project.clone(),
                observe: false,
            });
        }
        if at > 0 {
            writeln!(out)?;
        }
        let code = command.run(out, err)?;
        if code != 0 && code != crate::cli::strict::STRICT_EXIT_CODE {
            return Ok(code);
        }
    }
    Ok(0)
}

/// A tárolt parancs-fajta neve → a statisztika fajtája.
fn command_kind_named(name: &str) -> crate::stats::CommandKind {
    use crate::stats::CommandKind;
    match name {
        "locate" => CommandKind::Locate,
        "profile" => CommandKind::Profile,
        "show" => CommandKind::Show,
        _ => CommandKind::Other,
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
