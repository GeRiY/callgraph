//! A `telemetry` parancs kapcsolói: `callgraph telemetry on|off|status|preview` (és a belső `send` / `ask`:
//! a háttér-beküldés, illetve a telepítés / frissítés végi kérdés).

use crate::error::{CallgraphError, Result};

use super::CliArgs;

/// A `telemetry` alparancsa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryAction {
    On,
    Off,
    Status,
    Preview,
    /// Vissza a „még nem döntött” állapotba (be nem kapcsol).
    Reset,
    /// Belső: a leválasztott háttér-beküldés.
    Send,
    /// Belső: a telepítés / frissítés végi kérdés (csak interaktív terminálban, ha még nem döntött).
    Ask,
}

fn telemetry_usage() -> String {
    crate::t!(
        "callgraph telemetry status|preview|on|off|reset  (opt-in sending of the local usage counts; only the user can turn it on; reset = undecided again)",
        "callgraph telemetry status|preview|on|off|reset  (a helyi használati számok opt-in beküldése; csak a felhasználó kapcsolhatja be; reset = újra döntetlen)"
    )
    .to_owned()
}

/// `telemetry <action>`.
pub(crate) fn parse_telemetry(args: &[String]) -> Result<CliArgs> {
    let action = match args {
        [] => TelemetryAction::Status,
        [one] => match one.as_str() {
            "on" => TelemetryAction::On,
            "off" => TelemetryAction::Off,
            "status" => TelemetryAction::Status,
            "preview" => TelemetryAction::Preview,
            "reset" => TelemetryAction::Reset,
            "send" => TelemetryAction::Send,
            "ask" => TelemetryAction::Ask,
            _ => return Err(CallgraphError::Usage(telemetry_usage())),
        },
        _ => return Err(CallgraphError::Usage(telemetry_usage())),
    };
    Ok(CliArgs::Telemetry { action })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_telemetry_actions() {
        let parse =
            |v: &[&str]| parse_telemetry(&v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
        assert_eq!(
            parse(&["on"]).unwrap(),
            CliArgs::Telemetry {
                action: TelemetryAction::On
            }
        );
        assert_eq!(
            parse(&[]).unwrap(),
            CliArgs::Telemetry {
                action: TelemetryAction::Status
            }
        );
        assert!(parse(&["maybe"]).is_err());
        assert!(parse(&["on", "off"]).is_err());
    }
}
