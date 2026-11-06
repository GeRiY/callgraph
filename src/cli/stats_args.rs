//! A `stats` parancs kapcsolói: `callgraph stats [--days N]` (1–90, alapból 7). Külön fájlban, hogy az `args.rs`
//! csak egy sorral bővüljön.

use crate::error::{CallgraphError, Result};
use crate::stats::{DEFAULT_DAYS, MAX_DAYS};

use super::CliArgs;

fn stats_usage() -> String {
    crate::tf!(
        "callgraph stats [--days N]  (local usage stats of the last N days, 1–{MAX_DAYS}, default {DEFAULT_DAYS}; nothing is sent anywhere)",
        "callgraph stats [--days N]  (helyi használati statisztika az elmúlt N napról, 1–{MAX_DAYS}, alapból {DEFAULT_DAYS}; semmi nem megy el a gépről)"
    )
}

/// `stats [--days N]`.
pub(crate) fn parse_stats(args: &[String]) -> Result<CliArgs> {
    let mut days = DEFAULT_DAYS;
    let mut i = 0;
    while i < args.len() {
        let value = match args[i].as_str() {
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
    Ok(CliArgs::Stats { days })
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
            CliArgs::Stats { days: DEFAULT_DAYS }
        );
        assert_eq!(
            parse_stats(&s(&["--days", "30"])).unwrap(),
            CliArgs::Stats { days: 30 }
        );
        assert_eq!(
            parse_stats(&s(&["--days=90"])).unwrap(),
            CliArgs::Stats { days: 90 }
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
}
