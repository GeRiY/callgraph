//! A `uses` parancs kapcsolói: `callgraph uses <csomag> [--calls] [--project <alias|mappa>]` (hol és hogyan használja
//! a projekt egy függőségét, `crate::packages::UsageQuery`). Külön fájlban, hogy az `args.rs` csak egy sorral
//! bővüljön.

use crate::error::{CallgraphError, Result};

use super::CliArgs;

/// A parancs használata (a CLI és a hibaüzenet közös szövege).
pub(crate) fn uses_usage() -> &'static str {
    crate::t!(
        "callgraph uses <package> [--calls] [--project <alias|dir>]  (where the code uses a composer / npm dependency: per API symbol the count, the first file and +N; --calls: every call line per file)",
        "callgraph uses <csomag> [--calls] [--project <alias|mappa>]  (hol használja a kód egy composer / npm függőséget: API-szimbólumonként a darabszám, az első fájl és +N; --calls: fájlonként minden hívás-sor)"
    )
}

/// `uses <csomag> [--calls] [--project <alias|mappa>]`.
pub(crate) fn parse_uses(args: &[String]) -> Result<CliArgs> {
    let usage = || CallgraphError::Usage(uses_usage().into());
    let mut package: Option<String> = None;
    let mut calls = false;
    let mut project = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--calls" => calls = true,
            "--project" if i + 1 < args.len() => {
                project = Some(args[i + 1].clone());
                i += 1;
            }
            x if x.starts_with("--project=") => project = Some(x["--project=".len()..].to_owned()),
            x if x.starts_with('-') => return Err(usage()),
            x if package.is_none() && !x.trim().is_empty() => package = Some(x.trim().to_owned()),
            _ => return Err(usage()),
        }
        i += 1;
    }
    let Some(package) = package else {
        return Err(usage());
    };
    if project.as_deref().is_some_and(str::is_empty) {
        return Err(usage());
    }
    Ok(CliArgs::Uses {
        package,
        calls,
        project,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn parses_uses_package_calls_and_project() {
        assert_eq!(
            parse_uses(&s(&["guzzle"])).unwrap(),
            CliArgs::Uses {
                package: "guzzle".into(),
                calls: false,
                project: None
            }
        );
        assert_eq!(
            parse_uses(&s(&["--calls", "@vueuse/core", "--project", "shop"])).unwrap(),
            CliArgs::Uses {
                package: "@vueuse/core".into(),
                calls: true,
                project: Some("shop".into())
            }
        );
        assert_eq!(
            parse_uses(&s(&["guzzlehttp/guzzle", "--project=/x"])).unwrap(),
            CliArgs::Uses {
                package: "guzzlehttp/guzzle".into(),
                calls: false,
                project: Some("/x".into())
            }
        );
        assert!(parse_uses(&[]).is_err());
        for bad in [
            &["--calls"][..],
            &["a", "b"],
            &["a", "--all"],
            &["a", "--project"],
            &["a", "--project="],
            &[" "],
        ] {
            assert!(parse_uses(&s(bad)).is_err(), "{bad:?}");
        }
    }
}
