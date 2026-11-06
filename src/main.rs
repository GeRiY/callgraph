//! `callgraph` CLI-belépő: argumentumok → Command → futtatás; körülötte a helyi használati mérés (`stats::CliProbe`).

use std::io::{BufWriter, Write};
use std::process::ExitCode;

use callgraph::cli::{self, CliArgs};
use callgraph::stats::{CliProbe, Meter};

// a musl beépített allokátora sok szálon (rayon) zár-versengésbe fut: mimalloc minden platformon
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    callgraph::update::cleanup_stale_binary();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let probe = CliProbe::start(&args);
    let command = match CliArgs::parse(args).and_then(cli::build_command) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            probe.failed(&e);
            return ExitCode::from(2);
        }
    };
    // zár nélkül: a `callgraph mcp` a stdout-ot más szálról (tokio) írja, egy itt tartott zár holtpontot okozna
    let mut out = Meter::new(BufWriter::new(std::io::stdout()));
    let mut err = Meter::new(std::io::stderr());
    let result = command.run(&mut out, &mut err);
    let code = match &result {
        Ok(code) => *code,
        Err(e) => {
            let _ = writeln!(err, "{e}");
            1
        }
    };
    let _ = out.flush();
    probe.finish(&result, &out, &err);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
