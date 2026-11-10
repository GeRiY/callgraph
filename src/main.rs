//! `callgraph` CLI-belépő: argumentumok → Command (opt-in javaslat-sorral, `next::decorate`) → futtatás; körülötte a helyi használati mérés (`stats::CliProbe`).

use std::io::{BufWriter, Write};
use std::process::ExitCode;

use callgraph::cli::{self, CliArgs};
use callgraph::stats::{CliProbe, Meter};

// a musl beépített allokátora sok szálon (rayon) zár-versengésbe fut: mimalloc minden platformon
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if cli::take_no_gitignore(&mut args) {
        // SAFETY: a folyamat elején, minden más szál indulása előtt (egyszálú pont); a bejáró és a
        // gyermek-folyamatok (háttér-`refresh`) ebből tudják, hogy a `.gitignore` ki van kapcsolva
        unsafe {
            std::env::set_var(callgraph::io::GITIGNORE_ENV, "0");
        }
    }
    if cli::strict::take_strict(&mut args) {
        // SAFETY: mint fent — egyszálú pont; a `show` / `locate` / `profile` (és a `wrap`) ebből tudja, hogy a
        // szintaktikai hibás fájl 3-as kilépési kódot ad
        unsafe {
            std::env::set_var(cli::strict::STRICT_ENV, "1");
        }
    }
    if callgraph::next::take_next(&mut args) {
        // SAFETY: mint fent — egyszálú pont; a javaslat-sor (és a `callgraph mcp --next`) ebből tudja, hogy be van
        // kapcsolva
        unsafe {
            std::env::set_var(callgraph::next::NEXT_ENV, "1");
        }
    }
    callgraph::update::cleanup_stale_binary();
    let probe = CliProbe::start(&args);
    let command = match CliArgs::parse(args.clone()).and_then(cli::build_command) {
        Ok(c) => callgraph::next::decorate(&args, c),
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
    if matches!(result, Ok(0))
        && let Some(line) = probe.agent_hint()
    {
        let _ = writeln!(out, "{line}");
    }
    let _ = out.flush();
    probe.finish(&result, &out, &err);
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
