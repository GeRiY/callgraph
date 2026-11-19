//! The `trace` command's switches: `callgraph trace <target> [--project <alias|dir>]` (the task slice of a route,
//! controller action or page). A separate file so `args.rs` only grows by a line or two.

use crate::error::{CallgraphError, Result};

use super::CliArgs;

/// The command usage (shared by the CLI and the error message).
pub(crate) fn trace_usage() -> &'static str {
    crate::t!(
        "callgraph trace <target> [--project <alias|dir>]  (one compact chain, at most 30 lines: route -> middleware -> controller action -> FormRequest rules -> models -> Inertia page and its props -> child components; target: route::name, route:GET:/uri, a URI, Controller::method, Pages/X.vue or livewire:counter)",
        "callgraph trace <cél> [--project <alias|mappa>]  (egy tömör lánc, legfeljebb 30 sor: route -> middleware -> controller-akció -> FormRequest-szabályok -> modellek -> Inertia-oldal és props-ai -> gyerek-komponensek; cél: route::név, route:GET:/uri, egy URI, Controller::metódus, Pages/X.vue vagy livewire:counter)"
    )
}

/// `trace <target> [--project <alias|dir>]`.
pub(crate) fn parse_trace(args: &[String]) -> Result<CliArgs> {
    let usage = || CallgraphError::Usage(trace_usage().into());
    let mut target: Option<String> = None;
    let mut project = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--project" if i + 1 < args.len() => {
                project = Some(args[i + 1].clone());
                i += 1;
            }
            x if x.starts_with("--project=") => project = Some(x["--project=".len()..].to_owned()),
            x if x.starts_with('-') => return Err(usage()),
            x if target.is_none() && !x.trim().is_empty() => target = Some(x.trim().to_owned()),
            _ => return Err(usage()),
        }
        i += 1;
    }
    let Some(target) = target else {
        return Err(usage());
    };
    if project.as_deref().is_some_and(str::is_empty) {
        return Err(usage());
    }
    Ok(CliArgs::Trace { target, project })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn parses_target_and_project() {
        assert_eq!(
            parse_trace(&s(&["route::password.reset"])).unwrap(),
            CliArgs::Trace {
                target: "route::password.reset".into(),
                project: None
            }
        );
        assert_eq!(
            parse_trace(&s(&["--project=/x", "EventController::show"])).unwrap(),
            CliArgs::Trace {
                target: "EventController::show".into(),
                project: Some("/x".into())
            }
        );
        for bad in [
            &[][..],
            &["a", "b"],
            &["--all"],
            &["a", "--project"],
            &[" "],
        ] {
            assert!(parse_trace(&s(bad)).is_err(), "{bad:?}");
        }
    }
}
