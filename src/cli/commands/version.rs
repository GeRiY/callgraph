//! `callgraph --version` / `-V` — a verzió és a platform (`callgraph 0.1.0 (linux-x86_64)`); a develop-ág CI-buildje
//! (dev-csatorna) a `CALLGRAPH_BUILD_VERSION`-t írja ki: `callgraph 0.5.1-dev.23ed277 (linux-x86_64)`.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::update::{Platform, build_version};

/// A verzió-kiíró parancs.
pub struct VersionCommand;

impl Command for VersionCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        writeln!(
            out,
            "callgraph {} ({})",
            build_version(),
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
        assert!(text.starts_with(&format!("callgraph {} (", build_version())));
        assert!(
            text.trim_end()
                .ends_with(&format!("{})", Platform::current().dir()))
        );
    }
}
