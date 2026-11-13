//! `callgraph uses <csomag> [--calls] [--project <alias|út>]`: hol és hogyan használja a projekt egy függőségét — a
//! csomagtérképből (`crate::packages`) és a projekt-gráf kódfájljaiból (a `locate` forrása: újraelemzés csak a
//! változott fájloknál). Nem talált / nem használt csomag: egy sor, kilépési kód 0.

use std::io::Write;

use crate::analysis::Analyzer;
use crate::cli::Command;
use crate::error::Result;
use crate::locate::ProjectSource;
use crate::packages::UsageQuery;
use crate::skill::SkillHome;

/// A csomag-használat parancs; a `--project` aliast a `SkillHome` registryje oldja fel.
pub struct UsesCommand {
    package: String,
    calls: bool,
    project: Option<String>,
    home: SkillHome,
}

impl UsesCommand {
    pub fn new(package: String, calls: bool, project: Option<String>, home: SkillHome) -> Self {
        Self {
            package,
            calls,
            project,
            home,
        }
    }
}

impl Command for UsesCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let lang = crate::i18n::lang();
        let packages = crate::packages::open(&root);
        let query = UsageQuery::new(&packages, &root, &self.package);
        let text = if query.needs_files() {
            let analyzer = Analyzer::with_defaults();
            ProjectSource::new(&root, &analyzer).with_files(|files| {
                let code = files
                    .iter()
                    .filter(|file| file.code)
                    .map(|file| (file.path, file.graph));
                query.render(code, self.calls, lang)
            })?
        } else {
            query.render(std::iter::empty(), self.calls, lang)
        };
        out.write_all(text.as_bytes())?;
        Ok(0)
    }
}
