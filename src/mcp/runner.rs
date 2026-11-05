//! A lekérdezés futtatása: a projekt-gyökér feloldása (alias a `projects.json`-ból, vagy a munkakönyvtár), majd a
//! meglévő `show` parancs, a kimenete és a hibaszövege pufferbe gyűjtve.

use std::path::PathBuf;

use super::guard::ProjectRoot;
use super::request::ShowQuery;
use crate::cli::{CliArgs, build_command};
use crate::skill::{ProjectRegistry, SkillHome};

/// Egy `show`-lekérdezés végrehajtója (DI: a szerver ezen át fut, a teszt cserélheti).
pub(crate) trait QueryRunner: Send + Sync {
    /// A lekérdezés szöveges kimenete, vagy a (kétnyelvű) hibaüzenet.
    fn run(&self, query: &ShowQuery) -> Result<String, String>;
    /// A regisztrált projektek: `alias<TAB>gyökér` soronként.
    fn projects(&self) -> Result<String, String>;
}

/// Az éles végrehajtó: a skill-mappa registryje és a szerver munkakönyvtára.
pub(crate) struct ShowRunner {
    home: SkillHome,
    cwd: PathBuf,
}

impl ShowRunner {
    pub(crate) fn new(home: SkillHome, cwd: PathBuf) -> Self {
        Self { home, cwd }
    }

    fn registry(&self) -> Result<ProjectRegistry, String> {
        ProjectRegistry::load(self.home.registry_file()).map_err(|e| e.to_string())
    }

    /// Alias → a registry gyökere; alias nélkül a munkakönyvtár.
    fn root(&self, project: Option<&str>) -> Result<ProjectRoot, String> {
        let Some(alias) = project.filter(|a| !a.is_empty()) else {
            return ProjectRoot::new(&self.cwd, self.cwd.display().to_string());
        };
        let path = self.registry()?.root(alias).ok_or_else(|| {
            crate::tf!(
                "Unknown project alias: {alias} (see the 'projects' tool)",
                "Ismeretlen projekt-alias: {alias} (lásd a 'projects' eszközt)"
            )
        })?;
        ProjectRoot::new(&PathBuf::from(path), alias.to_owned())
    }
}

impl QueryRunner for ShowRunner {
    fn run(&self, query: &ShowQuery) -> Result<String, String> {
        let root = self.root(query.project.as_deref())?;
        let mut args = vec![
            "show".to_owned(),
            "--project".to_owned(),
            root.root().display().to_string(),
            "--label".to_owned(),
            root.label().to_owned(),
        ];
        args.extend(query.to_args(&root)?);
        let command = CliArgs::parse(args)
            .and_then(build_command)
            .map_err(|e| e.to_string())?;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = command.run(&mut out, &mut err).map_err(|e| e.to_string())?;
        if code != 0 {
            return Err(String::from_utf8_lossy(&err).trim_end().to_owned());
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn projects(&self) -> Result<String, String> {
        let registry = self.registry()?;
        let lines: Vec<String> = registry
            .list_lines()
            .iter()
            .filter_map(|line| line.split_once('\t').map(|(alias, _)| alias))
            .filter_map(|alias| registry.root(alias).map(|root| format!("{alias}\t{root}")))
            .collect();
        if lines.is_empty() {
            return Ok(crate::tf!(
                "No registered projects (registry: {}). Paths are resolved against the server's working directory: {}\n",
                "Nincs regisztrált projekt (registry: {}). Az utak a szerver munkakönyvtárához képest értendők: {}\n",
                self.home.registry_file().display(),
                self.cwd.display()
            ));
        }
        Ok(lines.join("\n") + "\n")
    }
}
