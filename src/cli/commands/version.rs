//! `callgraph --version` / `-V` — a verzió és a platform (`callgraph 0.1.0 (linux-x86_64)`).

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::update::Platform;

/// A verzió-kiíró parancs.
pub struct VersionCommand;

impl Command for VersionCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        writeln!(
            out,
            "callgraph {} ({})",
            env!("CARGO_PKG_VERSION"),
            Platform::current().dir()
        )?;
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_version_and_platform() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(VersionCommand.run(&mut out, &mut err).unwrap(), 0);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with(&format!("callgraph {} (", env!("CARGO_PKG_VERSION"))));
        assert!(
            text.trim_end()
                .ends_with(&format!("{})", Platform::current().dir()))
        );
    }
}
