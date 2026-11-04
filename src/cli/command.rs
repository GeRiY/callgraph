//! A CLI-parancsok közös felülete (Command minta).

use std::io::Write;

use crate::error::Result;

/// Egy futtatható CLI-alparancs; a kimenet és a hibacsatorna befecskendezett (tesztelhető).
pub trait Command {
    /// Lefuttatja a parancsot; a visszatérési érték a folyamat kilépési kódja.
    fn run(&self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32>;
}
