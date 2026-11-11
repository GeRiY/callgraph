//! `callgraph --help` / `-h` / `help` — a teljes súgó, a `skill/help/callgraph.{en,hu}.txt`-ből (egyetlen forrás:
//! a `callgraph.sh --help` is ugyanezt a fájlt írja ki). A szövegben a burok hívása (`bash …/callgraph.sh`) a
//! bináris saját nevére cserélődik.

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;

const HELP_EN: &str = include_str!("../../../skill/help/callgraph.en.txt");
const HELP_HU: &str = include_str!("../../../skill/help/callgraph.hu.txt");
const WRAPPER: &str = "bash ~/.claude/skills/callgraph/callgraph.sh";

/// A súgó-kiíró parancs.
pub struct HelpCommand;

impl HelpCommand {
    /// A súgó szövege az aktuális nyelven, a burok-hívás `callgraph`-ra cserélve.
    pub fn text() -> String {
        crate::t!(HELP_EN, HELP_HU).replace(WRAPPER, "callgraph")
    }
}

impl Command for HelpCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        write!(out, "{}", Self::text())?;
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_the_help_with_the_binary_name() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(HelpCommand.run(&mut out, &mut err).unwrap(), 0);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("callgraph locate <"), "{s}");
        assert!(!s.contains("callgraph.sh <"), "{s}");
    }
}
