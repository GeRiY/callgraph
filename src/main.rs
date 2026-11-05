//! `callgraph` CLI-belépő: argumentumok → Command → futtatás.

use std::io::{BufWriter, Write};
use std::process::ExitCode;

use callgraph::cli::{self, CliArgs};

fn main() -> ExitCode {
    callgraph::update::cleanup_stale_binary();
    let command = match CliArgs::parse(std::env::args().skip(1)).and_then(cli::build_command) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    // zár nélkül: a `callgraph mcp` a stdout-ot más szálról (tokio) írja, egy itt tartott zár holtpontot okozna
    let mut out = BufWriter::new(std::io::stdout());
    let mut err = std::io::stderr();
    let code = match command.run(&mut out, &mut err) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(err, "{e}");
            1
        }
    };
    let _ = out.flush();
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
