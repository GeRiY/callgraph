//! A 2. fázis helyi rangsorolója. A szabályok (`rules`) jelöltjeit csak SORBA RENDEZI és SZŰRI a helyi naplóból
//! (`log.jsonl` + `log.1.jsonl`) számolt, simított elfogadási pontszám szerint; újat nem talál ki, véletlen és
//! élő kísérletezés nincs. Alapból KI: `CALLGRAPH_NEXT_LEARN=1` (a `CALLGRAPH_NEXT` mellett).
//!
//! Pontszám egy (parancs, fajta) párra, a jelölt eredeti helyével (`pos`, 0-tól):
//! - hasznosság egy megjelenésre: elfogadva, visszalépés nélkül `+1`; elfogadva, visszalépéssel `-1`;
//!   nem elfogadva gyenge negatív, `-WEAK / (pos + 1)` (a hátsó helyek kevésbé informatívak);
//! - előzetes (a szabály-sorrend): `prior = -PRIOR_STEP * pos`;
//! - `score = (PRIOR_WEIGHT * prior + Σ hasznosság) / (PRIOR_WEIGHT + n)`.
//!
//! Kevés adatnál (`n < MIN_EVIDENCE` a (parancs, fajta) és a fajta szintjén is) a pontszám a `prior`, azaz a sorrend
//! pontosan a szabály-sorrend. Eldobás: `n >= DROP_EVIDENCE` és `score < DROP_BELOW`. Ha minden jelölt kiesik, nincs sor.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::log::{EventLog, LogEvent};
use super::suggestion::{Suggestion, SuggestionKind};

/// A tanulás környezeti változója (`1` / `true` / `yes` / `on`: be; csak a `CALLGRAPH_NEXT` mellett hat).
pub const LEARN_ENV: &str = "CALLGRAPH_NEXT_LEARN";
/// Az előzetes súlya (hány „virtuális” megjelenés).
const PRIOR_WEIGHT: f64 = 20.0;
/// Az előzetes lépése helyenként (a szabály-sorrend).
const PRIOR_STEP: f64 = 0.05;
/// A nem elfogadott megjelenés gyenge büntetése az első helyen.
const WEAK: f64 = 0.05;
/// Ennyi megjelenés kell, hogy az adat számítson a sorrendben.
pub(crate) const MIN_EVIDENCE: u32 = 10;
/// Ennyi megjelenés kell az eldobáshoz.
pub(crate) const DROP_EVIDENCE: u32 = 20;
/// Az eldobás küszöbe (a simított pontszám ez alatt).
const DROP_BELOW: f64 = -0.15;
/// Ennyi `shown` esemény alatt a kiértékelés még nem megbízható.
pub(crate) const MIN_EVAL_SHOWN: usize = 30;

/// Be van-e kapcsolva a tanulás (a `next` bekapcsolt állapota mellett).
pub(crate) fn learning() -> bool {
    super::enabled() && std::env::var(LEARN_ENV).is_ok_and(|v| super::is_on(&v))
}

/// A jelöltek rangsorolása a helyi napló alapján, ha `learn`; különben változatlanul. Üres eredmény: nincs sor.
pub(crate) fn apply_with(
    learn: bool,
    dir: &Path,
    cmd: &str,
    suggestions: Vec<Suggestion>,
) -> Vec<Suggestion> {
    if !learn || suggestions.is_empty() {
        return suggestions;
    }
    Model::from_events(&EventLog::new(dir).read_all()).rank(cmd, suggestions)
}

fn weak(pos: usize) -> f64 {
    WEAK / (pos as f64 + 1.0)
}

#[derive(Debug, Default, Clone, Copy)]
struct Stat {
    n: u32,
    sum: f64,
}

#[derive(Debug)]
struct Last {
    cmd: String,
    kind: SuggestionKind,
    flagged: bool,
}

/// A naplóból épített modell (soronként léptethető, a kiértékelés is így játssza vissza).
#[derive(Debug, Default)]
pub(crate) struct Model {
    by_ctx: HashMap<(String, SuggestionKind), Stat>,
    by_kind: HashMap<SuggestionKind, Stat>,
    shown: HashMap<u64, (String, Vec<SuggestionKind>)>,
    taken: HashSet<(u64, usize)>,
    last: Option<Last>,
}

impl Model {
    pub(crate) fn from_events(events: &[LogEvent]) -> Self {
        let mut model = Self::default();
        for event in events {
            model.observe(event);
        }
        model
    }

    fn add(&mut self, cmd: &str, kind: SuggestionKind, n: u32, delta: f64) {
        let ctx = self.by_ctx.entry((cmd.to_owned(), kind)).or_default();
        ctx.n += n;
        ctx.sum += delta;
        let all = self.by_kind.entry(kind).or_default();
        all.n += n;
        all.sum += delta;
    }

    /// Egy esemény beépítése (időrendben).
    pub(crate) fn observe(&mut self, event: &LogEvent) {
        match event {
            LogEvent::Shown { id, cmd, kinds, .. } => {
                for (pos, &kind) in kinds.iter().enumerate() {
                    self.add(cmd, kind, 1, -weak(pos));
                }
                self.shown.insert(*id, (cmd.clone(), kinds.clone()));
            }
            LogEvent::Accepted {
                id, choice, kind, ..
            } => {
                let Some((cmd, kinds)) = self.shown.get(id) else {
                    return;
                };
                let pos = choice.saturating_sub(1);
                if kinds.get(pos) != Some(kind) || !self.taken.insert((*id, *choice)) {
                    return;
                }
                let cmd = cmd.clone();
                // a megjelenéskor levont gyenge büntetés helyett +1
                self.add(&cmd, *kind, 0, 1.0 + weak(pos));
                self.last = Some(Last {
                    cmd,
                    kind: *kind,
                    flagged: false,
                });
            }
            LogEvent::Step { backtrack, .. } => {
                if *backtrack
                    && let Some(last) = self.last.as_mut()
                    && !last.flagged
                {
                    last.flagged = true;
                    let (cmd, kind) = (last.cmd.clone(), last.kind);
                    // +1 → -1
                    self.add(&cmd, kind, 0, -2.0);
                }
            }
        }
    }

    /// Pontszám és „eldobandó-e” egy jelöltre (eredeti hely: `pos`).
    fn score(&self, cmd: &str, kind: SuggestionKind, pos: usize) -> (f64, bool) {
        let prior = -PRIOR_STEP * pos as f64;
        let stat = self
            .by_ctx
            .get(&(cmd.to_owned(), kind))
            .filter(|s| s.n >= MIN_EVIDENCE)
            .or_else(|| self.by_kind.get(&kind).filter(|s| s.n >= MIN_EVIDENCE));
        match stat {
            None => (prior, false),
            Some(s) => {
                let score = (PRIOR_WEIGHT * prior + s.sum) / (PRIOR_WEIGHT + f64::from(s.n));
                (score, s.n >= DROP_EVIDENCE && score < DROP_BELOW)
            }
        }
    }

    /// A jelöltek új sorrendje: (eredeti index, eldobandó-e); stabil, azonos pontszámnál a szabály-sorrend marad.
    pub(crate) fn order(&self, cmd: &str, kinds: &[SuggestionKind]) -> Vec<(usize, bool)> {
        let mut scored: Vec<(usize, f64, bool)> = kinds
            .iter()
            .enumerate()
            .map(|(at, &kind)| {
                let (score, drop) = self.score(cmd, kind, at);
                (at, score, drop)
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.into_iter().map(|(at, _, drop)| (at, drop)).collect()
    }

    /// A jelöltek átrendezve, az eldobandók nélkül.
    pub(crate) fn rank(&self, cmd: &str, suggestions: Vec<Suggestion>) -> Vec<Suggestion> {
        let kinds: Vec<SuggestionKind> = suggestions.iter().map(|s| s.kind).collect();
        let mut slots: Vec<Option<Suggestion>> = suggestions.into_iter().map(Some).collect();
        self.order(cmd, &kinds)
            .into_iter()
            .filter(|(_, drop)| !drop)
            .filter_map(|(at, _)| slots[at].take())
            .collect()
    }
}

/// A kiértékelés számai.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Eval {
    pub events: usize,
    pub shown: usize,
    pub accepted: usize,
    pub hit_logged: usize,
    pub hit_learned: usize,
    pub rr_logged: f64,
    pub rr_learned: f64,
    pub backtracks: usize,
}

/// Visszajátszás időrendben: minden `shown`-nál csak az addigi eseményekből rangsorol; az elfogadott javaslat
/// helye a naplózott (szabály-) sorrendben és a tanult sorrendben.
pub(crate) fn evaluate(events: &[LogEvent]) -> Eval {
    let mut eval = Eval {
        events: events.len(),
        ..Eval::default()
    };
    let mut model = Model::default();
    let mut learned: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut counted: HashSet<u64> = HashSet::new();
    let mut open = false;
    for event in events {
        match event {
            LogEvent::Shown { id, cmd, kinds, .. } => {
                eval.shown += 1;
                let order = model.order(cmd, kinds);
                learned.insert(*id, order.into_iter().map(|(at, _)| at).collect());
            }
            LogEvent::Accepted { id, choice, .. } => {
                let at = choice.saturating_sub(1);
                let learned_pos = learned
                    .get(id)
                    .and_then(|order| order.iter().position(|&i| i == at));
                if let Some(learned_pos) = learned_pos
                    && counted.insert(*id)
                {
                    eval.accepted += 1;
                    eval.hit_logged += usize::from(at == 0);
                    eval.hit_learned += usize::from(learned_pos == 0);
                    eval.rr_logged += 1.0 / (at as f64 + 1.0);
                    eval.rr_learned += 1.0 / (learned_pos as f64 + 1.0);
                    open = true;
                }
            }
            LogEvent::Step { backtrack, .. } => {
                if *backtrack && open {
                    eval.backtracks += 1;
                    open = false;
                }
            }
        }
        model.observe(event);
    }
    eval
}

fn pct(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "n/a".to_owned();
    }
    format!("{:.1}%", 100.0 * part as f64 / whole as f64)
}

fn mrr(sum: f64, whole: usize) -> String {
    if whole == 0 {
        return "n/a".to_owned();
    }
    format!("{:.2}", sum / whole as f64)
}

/// A kiértékelés rövid szövege (legfeljebb 5 sor).
pub(crate) fn render(eval: &Eval) -> String {
    let (events, shown, accepted) = (eval.events, eval.shown, eval.accepted);
    let (hit_a, hit_b) = (
        pct(eval.hit_logged, accepted),
        pct(eval.hit_learned, accepted),
    );
    let (rr_a, rr_b) = (
        mrr(eval.rr_logged, accepted),
        mrr(eval.rr_learned, accepted),
    );
    let back = pct(eval.backtracks, accepted);
    let mut lines = vec![
        crate::tf!(
            "next --eval: {events} log events, {shown} shown, {accepted} accepted (logged order = rule order if learning was off)",
            "next --eval: {events} naplóesemény, {shown} megjelenítés, {accepted} elfogadás (a naplózott sorrend = szabály-sorrend, ha a tanulás ki volt kapcsolva)"
        ),
        crate::tf!(
            "top-1 hit rate: {hit_a} logged order, {hit_b} learned order",
            "top-1 találati arány: {hit_a} naplózott sorrend, {hit_b} tanult sorrend"
        ),
        crate::tf!(
            "mean reciprocal rank: {rr_a} logged, {rr_b} learned",
            "átlagos reciprok rang: {rr_a} naplózott, {rr_b} tanult"
        ),
        crate::tf!(
            "backtrack rate of accepted: {back}",
            "visszalépés az elfogadottaknál: {back}"
        ),
    ];
    if shown < MIN_EVAL_SHOWN {
        let min = MIN_EVAL_SHOWN;
        lines.push(crate::tf!(
            "too little data (< {min} shown events): the numbers are not reliable yet",
            "kevés az adat (< {min} megjelenítés): a számok még nem megbízhatók"
        ));
    }
    lines.join("\n")
}

/// `callgraph next --eval`: a projekt naplójának kiértékelése.
pub(crate) fn run_eval(root: &Path, out: &mut dyn std::io::Write) -> crate::error::Result<i32> {
    let store = super::store::ResultStore::for_root(root);
    let events = EventLog::new(store.dir()).read_all();
    writeln!(out, "{}", render(&evaluate(&events)))?;
    Ok(0)
}
