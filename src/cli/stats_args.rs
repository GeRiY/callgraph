//! A `stats` parancs kapcsolói: `callgraph stats [--events] [--days N]` (1–90, alapból 7; az eseménynapló legfeljebb
//! 14 napot tart). Külön fájlban, hogy az `args.rs` csak egy sorral bővüljön.

use crate::error::{CallgraphError, Result};
use crate::stats::{DEFAULT_DAYS, MAX_DAYS};

use super::CliArgs;

fn stats_usage() -> String {
    crate::tf!(
        "callgraph stats [--events] [--days N]  (local usage stats of the last N days, 1–{MAX_DAYS}, default {DEFAULT_DAYS}; --events: the local event log, max 14 days, and the top command transitions; nothing is sent unless telemetry is on)",
        "callgraph stats [--events] [--days N]  (helyi használati statisztika az elmúlt N napról, 1–{MAX_DAYS}, alapból {DEFAULT_DAYS}; --events: a helyi eseménynapló, max. 14 nap, és a leggyakoribb parancs-átmenetek; semmi nem megy el, hacsak a telemetria nincs bekapcsolva)"
    )
}

/// `stats [--events] [--days N]`.
pub(crate) fn parse_stats(args: &[String]) -> Result<CliArgs> {
    let mut days = DEFAULT_DAYS;
    let mut events = false;
    let mut i = 0;
    while i < args.len() {
        let value = match args[i].as_str() {
            "--events" => {
                events = true;
                i += 1;
                continue;
            }
            "--days" if i + 1 < args.len() => {
                i += 2;
                args[i - 1].as_str()
            }
            value if value.starts_with("--days=") => {
                i += 1;
                &value["--days=".len()..]
            }
            _ => return Err(CallgraphError::Usage(stats_usage())),
        };
        days = value
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=MAX_DAYS).contains(n))
            .ok_or_else(|| CallgraphError::Usage(stats_usage()))?;
    }
    Ok(CliArgs::Stats { days, events })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn parses_stats_days() {
        assert_eq!(
            parse_stats(&[]).unwrap(),
            CliArgs::Stats {
                days: DEFAULT_DAYS,
                events: false
            }
        );
        assert_eq!(
            parse_stats(&s(&["--days", "30"])).unwrap(),
            CliArgs::Stats {
                days: 30,
                events: false
            }
        );
        assert_eq!(
            parse_stats(&s(&["--days=90"])).unwrap(),
            CliArgs::Stats {
                days: 90,
                events: false
            }
        );
        for bad in [
            &["--days", "0"][..],
            &["--days", "91"],
            &["--days", "x"],
            &["--days"],
            &["--bogus"],
        ] {
            assert!(parse_stats(&s(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn parses_stats_events() {
        assert_eq!(
            parse_stats(&s(&["--events"])).unwrap(),
            CliArgs::Stats {
                days: DEFAULT_DAYS,
                events: true
            }
        );
        assert_eq!(
            parse_stats(&s(&["--days", "3", "--events"])).unwrap(),
            CliArgs::Stats {
                days: 3,
                events: true
            }
        );
        assert!(parse_stats(&s(&["--events", "--days"])).is_err());
    }
}
