//! `callgraph deps [--project <alias|út>]`: a valószínűleg fölösleges, a hiányzó és a csak dev függőségek — a
//! csomagtérképből (`crate::packages`) és a projekt-gráf kódfájljaiból (a `uses` bejárása: újraelemzés csak a
//! változott fájloknál). Ritka, igény szerinti parancs; találat nélkül egy sor, a kilépési kód mindig 0.

use std::io::Write;

use crate::analysis::Analyzer;
use crate::cli::Command;
use crate::error::Result;
use crate::locate::ProjectSource;
use crate::packages::DepsQuery;
use crate::skill::SkillHome;

/// A függőség-vizsgálat parancs; a `--project` aliast a `SkillHome` registryje oldja fel.
pub struct DepsCommand {
    project: Option<String>,
    home: SkillHome,
}

impl DepsCommand {
    pub fn new(project: Option<String>, home: SkillHome) -> Self {
        Self { project, home }
    }
}

impl Command for DepsCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let root = super::project_root(&self.home, self.project.as_deref())?;
        let lang = crate::i18n::lang();
        let packages = crate::packages::open(&root);
        let query = DepsQuery::new(&packages, &root, crate::packages::user_ignores(&root));
        let text = if query.needs_files() {
            let analyzer = Analyzer::with_defaults();
            ProjectSource::new(&root, &analyzer).with_files(|files| {
                let code = files
                    .iter()
                    .filter(|file| file.code)
                    .map(|file| (file.path, file.graph));
                query.render(code, lang)
            })?
        } else {
            query.render(std::iter::empty(), lang)
        };
        out.write_all(text.as_bytes())?;
        Ok(0)
    }
}
