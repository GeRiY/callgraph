//! A háttér-ellenőrzés indítása: leválasztott gyerekfolyamat (`callgraph self-update --check --background`),
//! null stdin/stdout/stderr-rel; nem várunk rá (`spawn_detached`: a háttér-`refresh` is ezt használja). A `Spawner` trait mögött, hogy a tesztek ne indítsanak folyamatot.

use std::process::{Command, Stdio};

/// A háttér-ellenőrzés gyerekfolyamatának argumentumai.
pub(crate) const BACKGROUND_ARGS: [&str; 3] = ["self-update", "--check", "--background"];

/// Egy háttér-ellenőrzés elindítása (várakozás nélkül, a hibát elnyelve).
pub(crate) trait Spawner {
    fn spawn(&self);
}

/// A valódi indító: a futó bináris, leválasztva (Unixon saját folyamatcsoport, Windowson ablak és konzol nélkül).
pub(crate) struct DetachedProcess;

impl Spawner for DetachedProcess {
    fn spawn(&self) {
        spawn_detached(BACKGROUND_ARGS);
    }
}

/// A futó bináris indítása a megadott argumentumokkal, leválasztva, null stdin/stdout/stderr-rel, várakozás nélkül
/// (a hibát elnyelve; `false`, ha nem indult el). A háttér-ellenőrzés és a `callgraph refresh --background` /
/// `callgraph hook post-edit` közös indítója.
pub(crate) fn spawn_detached<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    spawn_detached_as(args, "background")
}

/// Mint a `spawn_detached`, a gyerek a `client` jelzőt kapja a helyi statisztikához (`stats::CLIENT_ENV`):
/// `hook` = a hook háttér-frissítése (mérjük), `background` = nem mérjük (a szülő már mért).
pub(crate) fn spawn_detached_as<I, S>(args: I, client: &str) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut command = Command::new(exe);
    command
        .args(args)
        .env(crate::stats::CLIENT_ENV, client)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    // a Child eldobása nem öli meg és nem várja meg a folyamatot
    command.spawn().is_ok()
}

/// Unix: saját folyamatcsoport — a hívó terminál / burok jelzései (Ctrl-C, csoport-kilövés) nem érik el.
#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

/// Windows: `DETACHED_PROCESS | CREATE_NO_WINDOW` — nincs konzol, nem villan fel ablak.
#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
fn detach(_command: &mut Command) {}
