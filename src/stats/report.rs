//! A `callgraph stats` szövege (tömör, ≤ ~20 sor): időszak, hívások kliensenként, parancsonként a hívások, a
//! hibák, az idő- és a kimenet-méret sávjai, a `locate`-találati arány, és hogy hol az adat, és hogy nem megy
//! sehova.

use std::fmt::Write as _;
use std::path::Path;

use super::buckets::{LatencyBucket, RankBucket, SizeBucket};
use super::day::Day;
use super::event::{Client, CommandKind};
use super::store::{Counters, Totals};

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
