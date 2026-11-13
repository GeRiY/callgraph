//! A lekérdezés futtatása: a projekt-gyökér feloldása (alias a `projects.json`-ból, vagy a munkakönyvtár), majd a
//! meglévő `show` (a `locate` eszköznél a `locate`) parancs, a kimenete és a hibaszövege pufferbe gyűjtve.

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
    /// A projekt-profil (`callgraph profile`) szövege; `project`: alias (`None` = a munkakönyvtár); `packages`: a
    /// közvetlen függőségek listája is (`--packages`).
    fn profile(&self, project: Option<&str>, full: bool, packages: bool) -> Result<String, String> {
        let _ = (project, full, packages);
        Err(crate::t!(
            "project_profile is not available",
            "a project_profile nem érhető el"
        )
        .into())
    }
    /// Az `initialize` útmutatójának vége: a munkakönyvtár tömör profilja, vagy tipp a `project_profile`-ra.
    fn startup_note(&self) -> String {
        String::new()
    }
    /// Egy korábbi eredmény javasolt következő lépése(i) (`next` eszköz, `crate::next`): `id` (`r7`), a választások.
    fn next(&self, project: Option<&str>, id: &str, choices: &[usize]) -> Result<String, String> {
        let _ = (project, choices);
        Err(unknown_result(id))
    }
}

/// Az ismeretlen / lejárt eredmény-azonosító üzenete.
pub(crate) fn unknown_result(id: &str) -> String {
    crate::tf!(
        "Unknown or expired result id: {id} — run the query again.",
        "Ismeretlen vagy lejárt eredmény-azonosító: {id} — futtasd újra a lekérdezést."
    )
}

/// Az éles végrehajtó: a skill-mappa registryje és a szerver munkakönyvtára.
pub(crate) struct ShowRunner {
    home: SkillHome,
    cwd: PathBuf,
    /// A munkakönyvtár tömör profilja (egyszer számolva, az `initialize`-kor).
    startup: std::sync::OnceLock<String>,
}

impl ShowRunner {
    pub(crate) fn new(home: SkillHome, cwd: PathBuf) -> Self {
        Self {
            home,
            cwd,
            startup: std::sync::OnceLock::new(),
        }
    }

    /// Egy CLI-parancs futtatása pufferbe: siker → a kimenet (bekapcsolt javaslatoknál a végén a javaslat-sorral,
    /// `crate::next`), hiba → a hibaszöveg.
    fn execute(args: Vec<String>, root: &ProjectRoot) -> Result<String, String> {
        let mut text = Self::execute_plain(args.clone())?;
        if crate::next::enabled() {
            let project = Some(root.label()).filter(|label| !label.contains('/'));
            if let Some(line) = crate::next::annotate(&args, root.root(), &text, project) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        Ok(text)
    }

    fn execute_plain(args: Vec<String>) -> Result<String, String> {
        let command = CliArgs::parse(args)
            .and_then(build_command)
            .map_err(|e| e.to_string())?;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = command.run(&mut out, &mut err).map_err(|e| e.to_string())?;
        // a `CALLGRAPH_STRICT` csak a CLI-é: itt a 3-as kód (szintaktikai hibás fájl) is siker, a `⚠` sor a kimenetben
        if code != 0 && code != crate::cli::strict::STRICT_EXIT_CODE {
            return Err(String::from_utf8_lossy(&err).trim_end().to_owned());
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
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
        let mut args = query.head(&root);
        args.extend(query.to_args(&root)?);
        Self::execute(args, &root)
    }

    fn profile(&self, project: Option<&str>, full: bool, packages: bool) -> Result<String, String> {
        let root = self.root(project)?;
        let mut args = vec![
            "profile".to_owned(),
            "--project".to_owned(),
            root.root().display().to_string(),
        ];
        if full {
            args.push("--full".into());
        }
        if packages {
            args.push("--packages".into());
        }
        Self::execute(args, &root)
    }

    fn next(&self, project: Option<&str>, id: &str, choices: &[usize]) -> Result<String, String> {
        let root = self.root(project)?;
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = crate::next::run_choices(root.root(), id, choices, &mut out, &mut err)
            .map_err(|e| e.to_string())?;
        if code != 0 && code != crate::cli::strict::STRICT_EXIT_CODE {
            return Err(String::from_utf8_lossy(&err).trim_end().to_owned());
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn startup_note(&self) -> String {
        self.startup
            .get_or_init(|| super::guide::startup_note(&self.cwd, crate::i18n::lang()))
            .clone()
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
