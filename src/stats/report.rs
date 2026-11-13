//! A `callgraph stats` szövege (tömör, ≤ ~20 sor): időszak, hívások kliensenként, parancsonként a hívások, a
//! hibák, az idő- és a kimenet-méret sávjai, a `locate`-találati arány, és hogy hol az adat, és hogy nem megy
//! sehova.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use super::buckets::{LatencyBucket, RankBucket, SizeBucket};
use super::day::Day;
use super::event::{Client, CommandKind};
use super::events::{EVENT_DAYS, EVENTS_DIR, EventLine, read as read_events};
use super::reuse::{FILE_BANDS, SYM_BANDS};
use super::store::{Counters, StatsStore, Totals};

/// A riport a `today`-jel végződő `days` napra (a `totals` erre az időszakra összegzett), a mappa, és hogy most
/// mérünk-e.
pub(crate) fn render(totals: &Totals, today: Day, days: u32, dir: &Path, enabled: bool) -> String {
    let (from, to) = (today.minus(days.saturating_sub(1)), today);
    let mut out = String::new();
    let tail = footer(dir, enabled);
    let calls = totals.calls();
    if calls == 0 {
        out.push_str(&crate::tf!(
            "callgraph stats: no usage recorded in the last {days} days.\n",
            "callgraph stats: az elmúlt {days} napban nincs rögzített használat.\n"
        ));
        out.push_str(&tail);
        return out;
    }
    let mut clients: Vec<(Client, u64)> = Client::ALL
        .iter()
        .map(|c| (*c, totals.client_calls(*c)))
        .filter(|(_, n)| *n > 0)
        .collect();
    clients.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let by_client = clients
        .iter()
        .map(|(c, n)| format!("{} {n}", c.name()))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&crate::tf!(
        "callgraph stats · last {days} days ({from} – {to}, UTC) · {calls} calls: {by_client}\n",
        "callgraph stats · az elmúlt {days} nap ({from} – {to}, UTC) · {calls} hívás: {by_client}\n"
    ));
    let time = LatencyBucket::LABELS.join(" / ");
    let size = SizeBucket::LABELS.join(" / ");
    out.push_str(&crate::tf!(
        "time: {time} · output (tokens): {size}\n",
        "idő: {time} · kimenet (token): {size}\n"
    ));
    table(&mut out, totals);
    if totals.commands.values().any(|c| c.error_count() > 0) {
        out.push_str(crate::t!(
            "user err = usage, not found, unsupported file (counted, not logged) · internal = also in errors.jsonl\n",
            "felh. hiba = használat, nincs meg, nem támogatott fájl (számolva, nem naplózva) · belső = az errors.jsonl-ben is\n"
        ));
    }
    if let Some(line) = totals.commands.get(&CommandKind::Locate).and_then(hit_line) {
        out.push_str(&line);
    }
    if let Some(line) = next_line(totals) {
        out.push_str(&line);
    }
    out.push_str(&tail);
    out
}

/// A parancs-táblázat: hívások szerint csökkenő sorrendben.
fn table(out: &mut String, totals: &Totals) {
    let mut rows: Vec<(CommandKind, &Counters)> = totals
        .commands
        .iter()
        .filter(|(_, c)| c.calls > 0)
        .map(|(k, c)| (*k, c))
        .collect();
    rows.sort_by(|a, b| b.1.calls.cmp(&a.1.calls).then(a.0.cmp(&b.0)));
    let cells: Vec<[String; 7]> = rows
        .iter()
        .map(|(kind, c)| {
            [
                kind.name().to_owned(),
                c.calls.to_string(),
                c.user_error_count().to_string(),
                c.internal_error_count().to_string(),
                slashed(&c.latency),
                slashed(&c.size),
                c.truncated.to_string(),
            ]
        })
        .collect();
    let header = [
        crate::t!("command", "parancs"),
        crate::t!("calls", "hívás"),
        crate::t!("user err", "felh. hiba"),
        crate::t!("internal", "belső"),
        crate::t!("time", "idő"),
        crate::t!("output", "kimenet"),
        crate::t!("truncated", "levágva"),
    ];
    let mut widths = header.map(|h| h.chars().count());
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let line = |cells: [&str; 7]| -> String {
        let [name, calls, user, internal, time, size, cut] = cells;
        format!(
            "{name:<w0$}  {calls:>w1$}  {user:>w2$}  {internal:>w3$}  {time:<w4$}  {size:<w5$}  {cut}\n",
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3],
            w4 = widths[4],
            w5 = widths[5],
        )
    };
    out.push_str(&line(header));
    for row in &cells {
        out.push_str(&line(row.each_ref().map(String::as_str)));
    }
}

/// `3/1/0/0/0`
fn slashed(values: &[u64]) -> String {
    let mut text = String::new();
    for (i, n) in values.iter().enumerate() {
        if i > 0 {
            text.push('/');
        }
        let _ = write!(text, "{n}");
    }
    text
}

/// A `locate`-találati arány sora, ha volt lezárt lista.
fn hit_line(locate: &Counters) -> Option<String> {
    let total: u64 = locate.hits.iter().sum();
    if total == 0 {
        return None;
    }
    let first = locate.hits[RankBucket::First.index()];
    let top5 = first + locate.hits[RankBucket::Top5.index()];
    let unlisted = locate.hits[RankBucket::NotListed.index()];
    Some(crate::tf!(
        "locate hits: first result was the one opened: {first} of {total}, top 5: {top5} of {total}, none of the list: {unlisted}\n",
        "locate-találat: az első találat volt a megnyitott fájl: {first} / {total}, az első 5 között: {top5} / {total}, egyik sem: {unlisted}\n"
    ))
}

/// A javaslatok (`next`) elfogadási aránya, ha volt javaslat-sor: összesen és parancsonként.
fn next_line(totals: &Totals) -> Option<String> {
    let shown: u64 = totals.commands.values().map(|c| c.next_shown).sum();
    if shown == 0 {
        return None;
    }
    let used: u64 = totals.commands.values().map(|c| c.next_used).sum();
    let per = totals
        .commands
        .iter()
        .filter(|(_, c)| c.next_shown > 0)
        .map(|(kind, c)| format!("{} {}/{}", kind.name(), c.next_used, c.next_shown))
        .collect::<Vec<_>>()
        .join(", ");
    let rate = used * 100 / shown;
    Some(crate::tf!(
        "next suggestions: used {used} of {shown} ({rate}%): {per}\n",
        "next-javaslatok: {used} / {shown} használva ({rate}%): {per}\n"
    ))
}

/// Az utolsó sor(ok): hol az adat, hogy nem megy sehova, és a kikapcsolás.
fn footer(dir: &Path, enabled: bool) -> String {
    let dir = dir.display();
    let mut text = crate::tf!(
        "Data: {dir} (stays local unless the user turned sending on: callgraph.sh telemetry status) · turn off: CALLGRAPH_STATS=0\n",
        "Adat: {dir} (helyben marad, hacsak a felhasználó be nem kapcsolta a beküldést: callgraph.sh telemetry status) · kikapcsolás: CALLGRAPH_STATS=0\n"
    );
    if !enabled {
        text.push_str(crate::t!(
            "Recording is off here (CALLGRAPH_STATS=0, CI, or not running from a skill install).\n",
            "A mérés itt ki van kapcsolva (CALLGRAPH_STATS=0, CI, vagy nem skill-telepítésből fut).\n"
        ));
    }
    text
}

/// A `stats --events` legfeljebb ennyi (legújabb) eseményt ír ki.
pub(crate) const MAX_EVENT_LINES: usize = 300;
/// A „leggyakoribb átmenetek” listájának hossza.
const TOP_EDGES: usize = 10;

/// `callgraph stats --events`: a helyi eseménynapló a `today`-jel végződő `days` napról (legújabb a végén,
/// tömören, eseményenként egy sor), és a napi élekből a leggyakoribb átmenetek.
pub(crate) fn render_events(store: &StatsStore, today: Day, days: u32) -> String {
    let days = days.clamp(1, EVENT_DAYS);
    let from = today.minus(days - 1);
    let events = read_events(store.dir(), from, today);
    let mut out = crate::tf!(
        "callgraph stats --events · last {days} days ({from} – {today}, UTC) · {} events (local only, never sent)\n",
        "callgraph stats --events · az elmúlt {days} nap ({from} – {today}, UTC) · {} esemény (csak helyben, soha nem megy el)\n",
        events.len()
    );
    let skipped = events.len().saturating_sub(MAX_EVENT_LINES);
    if skipped > 0 {
        out.push_str(&crate::tf!(
            "(the oldest {skipped} not shown)\n",
            "(a legrégebbi {skipped} nincs kiírva)\n"
        ));
    }
    if !events.is_empty() {
        out.push_str(crate::t!(
            "time (UTC) · session#call · via · command · result · ms · output · lang · input · relation · repeat · use (f: file, s: symbol in the previous output)\n",
            "idő (UTC) · munkamenet#hívás · kliens · parancs · eredmény · ms · kimenet · nyelv · bemenet · viszony · ismétlés · use (f: fájl, s: szimbólum az előző kimenetben)\n"
        ));
    }
    for line in &events[skipped..] {
        out.push_str(&event_row(line));
    }
    let edges = store.edge_totals(from, today);
    let mut top: Vec<(&String, &u64)> = edges.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    if !top.is_empty() {
        out.push_str(crate::t!(
            "top transitions (previous>next|relation|gap, ! = previous failed):\n",
            "leggyakoribb átmenetek (előző>következő|viszony|szünet, ! = az előző hibás volt):\n"
        ));
        for (key, n) in top.into_iter().take(TOP_EDGES) {
            let _ = writeln!(out, "  {n:>5}  {key}");
        }
    }
    let reuse = reuse_lines(&store.use_totals(from, today));
    if !reuse.is_empty() {
        out.push_str(crate::t!(
            "previous output reused (file: hit1 = first file listed, hit5 = 2nd–5th, hit = later, miss = not there):\n",
            "az előző kimenet felhasználása (fájl: hit1 = az első felsorolt, hit5 = 2.–5., hit = később, miss = nem volt benne):\n"
        ));
        for line in reuse {
            out.push_str(&line);
        }
    }
    let dir = store.dir().join(EVENTS_DIR);
    out.push_str(&crate::tf!(
        "Log: {} (kept {EVENT_DAYS} days, no paths or names; never sent)\n",
        "Napló: {} ({EVENT_DAYS} napig marad, út és név nélkül; soha nem megy el)\n",
        dir.display()
    ));
    out
}

/// Egy átmenet (`előző>következő`) újrahasznosítási számai.
#[derive(Default)]
struct Reuse<'a> {
    calls: u64,
    files: BTreeMap<&'a str, u64>,
    syms: BTreeMap<&'a str, u64>,
}

/// A napi `uses` kulcsaiból (`{előző}[!]>{ez}|f:{sáv}|s:{sáv}`) a leggyakoribb átmenetek soronként, a fájl- és a
/// szimbólum-sávok megoszlásával (a `-` sávok nélkül; az `other` kimarad).
fn reuse_lines(uses: &BTreeMap<String, u64>) -> Vec<String> {
    let mut pairs: BTreeMap<&str, Reuse<'_>> = BTreeMap::new();
    for (key, n) in uses {
        let mut parts = key.split('|');
        let (Some(pair), Some(file), Some(sym)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let entry = pairs.entry(pair).or_default();
        entry.calls += n;
        if let Some(band) = file.strip_prefix("f:").filter(|b| *b != "-") {
            *entry.files.entry(band).or_default() += n;
        }
        if let Some(band) = sym.strip_prefix("s:").filter(|b| *b != "-") {
            *entry.syms.entry(band).or_default() += n;
        }
    }
    let mut top: Vec<(&str, Reuse<'_>)> = pairs.into_iter().collect();
    top.sort_by(|a, b| b.1.calls.cmp(&a.1.calls).then(a.0.cmp(b.0)));
    top.into_iter()
        .take(TOP_EDGES)
        .map(|(pair, reuse)| {
            let pair = pair.replace('>', " → ");
            let n = reuse.calls;
            let files = split(&reuse.files, &FILE_BANDS);
            let syms = split(&reuse.syms, &SYM_BANDS);
            crate::tf!(
                "  {pair}: {n} (file from previous output: {files} · symbol: {syms})\n",
                "  {pair}: {n} (fájl az előző kimenetből: {files} · szimbólum: {syms})\n"
            )
        })
        .collect()
}

/// `hit1 10, hit5 3, miss 2` a sávok sorrendjében (üresen `-`).
fn split(counts: &BTreeMap<&str, u64>, bands: &[&str]) -> String {
    let parts: Vec<String> = bands
        .iter()
        .filter_map(|band| counts.get(band).map(|n| format!("{band} {n}")))
        .collect();
    if parts.is_empty() {
        "-".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Egy esemény egy sorban.
fn event_row(line: &EventLine) -> String {
    let dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".to_owned());
    let result = match line.err {
        None if line.ok => "ok".to_owned(),
        None => "err".to_owned(),
        Some(kind) => serde_json::to_string(&kind)
            .unwrap_or_default()
            .trim_matches('"')
            .to_owned(),
    };
    format!(
        "{}  s{}#{:<3} {:<4} {:<15} {:<11} {:>6}  {:<7} {:<5} {:<7} {:<13} {:<4} use f:{} s:{}\n",
        line.t0,
        line.sess,
        line.seq,
        line.via.name(),
        line.cmd.name(),
        result,
        line.ms,
        line.out,
        dash(&line.lang),
        dash(&line.input),
        line.rel.name(),
        dash(&line.rep),
        line.use_file,
        line.use_sym
    )
}
