//! yarn: a `yarn.lock` classic (v1) és berry alakja, kézzel írt sor-olvasóval (nem általános YAML). Egy blokk egy
//! behúzás nélküli fejléc-sorral kezdődik, amely a `név@tartomány` specifikátorokat sorolja (idézőjelben is,
//! vesszővel elválasztva; berryben `npm:` protokollal), és az első behúzási szintű `version` sor adja a verziót
//! (`version "7.1.0"` / `version: 7.1.0`). Minden bejegyzés visszajön; a közvetlen függőségre szűrés a builderé.

use super::LockSource;
use crate::packages::model::Installed;

/// A `yarn.lock` stratégiája.
pub(crate) struct YarnLock;

impl LockSource for YarnLock {
    fn file(&self) -> &'static str {
        "yarn.lock"
    }

    fn read(&self, text: &str) -> Vec<Installed> {
        let mut out: Vec<Installed> = Vec::new();
        let mut block: Vec<(String, String)> = Vec::new();
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !line.starts_with([' ', '\t']) {
                block = line.strip_suffix(':').map(specifiers).unwrap_or_default();
                continue;
            }
            // csak az első behúzási szint (két szóköz): a mélyebb sorok a blokk függőségei
            if block.is_empty() || line.starts_with("   ") {
                continue;
            }
            let Some(version) = version_of(line.trim_start()) else {
                continue;
            };
            for (name, range) in block.drain(..) {
                match out
                    .iter_mut()
                    .find(|entry| entry.name == name && entry.version == version)
                {
                    Some(entry) => entry.ranges.push(range),
                    None => out.push(Installed {
                        name,
                        version: version.to_owned(),
                        namespaces: Vec::new(),
                        ranges: vec![range],
                    }),
                }
            }
        }
        out
    }
}

/// A fejléc specifikátorai: (név, tartomány), az `npm:` előtag nélkül; a névtelen (`__metadata`) kimarad.
fn specifiers(header: &str) -> Vec<(String, String)> {
    header
        .split(',')
        .filter_map(|spec| split_spec(spec.trim().trim_matches('"')))
        .map(|(name, range)| {
            let range = range.strip_prefix("npm:").unwrap_or(range);
            (name.to_owned(), range.to_owned())
        })
        .collect()
}

/// `@vue/shared@^3.4` → (`@vue/shared`, `^3.4`): a név az első, nem a legelső helyen álló `@`-ig tart.
fn split_spec(spec: &str) -> Option<(&str, &str)> {
    let at = spec.get(1..)?.find('@')? + 1;
    Some((&spec[..at], &spec[at + 1..]))
}

/// `version "7.1.0"` / `version: 7.1.0` → `7.1.0`.
fn version_of(body: &str) -> Option<&str> {
    let rest = body.strip_prefix("version")?;
    let rest = rest.strip_prefix(':').unwrap_or(rest);
    if !rest.starts_with(' ') {
        return None;
    }
    let version = rest.trim().trim_matches('"');
    (!version.is_empty()).then_some(version)
}
