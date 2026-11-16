//! Láncszem: a JS / TS / Vue csomag-import (`axios`, `@vueuse/core`, `lodash/debounce`) → a csomag neve
//! (`@scope/név` vagy `név`, az al-út nélkül). A nem deklarált csomag is (az a „hiányzó függőség”), a verzióval, ha
//! a fájlhoz legközelebbi Node-rész lockja ismeri. A relatív és az alias-os (jsconfig / tsconfig `paths`) importot
//! már az elemzés kiszűri (`Call::from` csak a nem helyi forrásnál van); itt a maradék nem-csomag alakok esnek ki
//! (abszolút út, `~/…`, `#…`, `$lib`, `node:` / `virtual:` séma), a Node beépített moduljai pedig `Unknown`-ok.

use super::{Attribution, Resolution, SymbolRef, SymbolResolver};
use crate::packages::model::Ecosystem;
use crate::packages::project::EcosystemPackages;

/// A Node beépített moduljai (a `node:` előtag nélkül is így importálhatók): nem csomagok.
const BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// A csomag-import.
pub(super) struct BareImport;

impl SymbolResolver for BareImport {
    fn resolve(&self, sym: &SymbolRef<'_>, part: &EcosystemPackages) -> Option<Resolution> {
        if sym.ecosystem != Ecosystem::Node {
            return None;
        }
        let name = package_name(sym.qualifier)?;
        if is_builtin(name) {
            return Some(Resolution::Unknown);
        }
        let version = part
            .package(name)
            .and_then(|package| package.installed.clone());
        Some(Resolution::Package(Attribution {
            name: name.to_owned(),
            version,
        }))
    }
}

/// A Node beépített modulja-e a név (`fs`, `path`; a `node:` / `bun:` előtagos alak is).
pub(crate) fn is_builtin(name: &str) -> bool {
    name.starts_with("node:") || name.starts_with("bun:") || BUILTINS.contains(&name)
}

/// A specifikátor csomagneve: `@scope/név/al-út` → `@scope/név`, `név/al-út` → `név`; ha nem csomag alakú, `None`.
pub(crate) fn package_name(spec: &str) -> Option<&str> {
    if spec.contains([':', '\\']) {
        return None;
    }
    let mut segments = spec.split('/');
    let first = segments.next()?;
    let end = match first.strip_prefix('@') {
        Some(scope) => {
            let name = segments.next()?;
            if !valid_segment(scope) || !valid_segment(name) {
                return None;
            }
            first.len() + 1 + name.len()
        }
        None => {
            if !valid_segment(first) {
                return None;
            }
            first.len()
        }
    };
    spec.get(..end)
}

/// Egy csomagnév-tag: nem üres, betűvel / számjeggyel kezdődik (nem `.`, `_`, `~`, `#`, `$`), és csak az npm-nevek
/// karaktereit tartalmazza.
fn valid_segment(segment: &str) -> bool {
    segment
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~'))
}
