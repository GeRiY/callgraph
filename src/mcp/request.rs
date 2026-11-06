//! Eszköz-paraméter → `show`-lekérdezés leképezés. A lekérdezést a meglévő `show` parancs futtatja (ugyanaz a
//! motor, ugyanaz a tömör kimenet és kimeneti korlát, mint a CLI-ben) — itt csak az argumentumlista készül. A
//! `locate` eszköz ugyanígy a `locate` parancsot futtatja.

use super::guard::ProjectRoot;
use super::params::{
    FindCallersParams, FrameworkViewParams, LocateParams, MethodBodyParams, NotesParams,
    OutlineParams, SearchKind, SearchParams,
};

/// A `find_callers` megengedett mélysége.
const MAX_DEPTH: u8 = 5;

/// A futtatott CLI-parancs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Subcommand {
    /// `show` (minden eszköz, a `locate` kivételével).
    Show,
    /// `locate`: a kifejezések a cél helyett.
    Locate,
}

/// Egy `show`- (vagy `locate`-) hívás leírása, a gyökér feloldása előtt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShowQuery {
    /// Projekt-alias (`None` = a szerver munkakönyvtára).
    pub project: Option<String>,
    command: Subcommand,
    /// A cél (fájl vagy mappa), a gyökérhez képest.
    path: String,
    /// `::metódus` szelektorok a célhoz (üres = az egész cél).
    methods: Vec<String>,
    /// A `show` kapcsolói (`--format`, `--grep` …).
    flags: Vec<String>,
}

impl ShowQuery {
    fn new(project: Option<String>, path: String, full: bool) -> Self {
        let flags = if full {
            vec!["--full".into()]
        } else {
            Vec::new()
        };
        Self {
            project,
            command: Subcommand::Show,
            path,
            methods: Vec::new(),
            flags,
        }
    }

    fn flag(mut self, name: &str, value: impl Into<String>) -> Self {
        self.flags.push(name.into());
        self.flags.push(value.into());
        self
    }

    /// `--layer <név>`, ha meg van adva (üres = nincs szűrő).
    fn layer(self, layer: Option<String>) -> Self {
        match layer.filter(|l| !l.trim().is_empty()) {
            Some(layer) => self.flag("--layer", layer),
            None => self,
        }
    }

    /// `outline`: `--format <f>` és opcionálisan `<út>::<metódus>`.
    pub(crate) fn outline(p: OutlineParams) -> Result<Self, String> {
        let format = p.format.unwrap_or_default().as_str();
        let mut query = Self::new(p.project, p.path, p.full)
            .flag("--format", format)
            .layer(p.layer);
        if let Some(method) = p.method.filter(|m| !m.is_empty()) {
            query.methods.push(method);
        }
        Ok(query)
    }

    /// `method_body`: `<út>::<m1> <út>::<m2> … --body`.
    pub(crate) fn method_body(p: MethodBodyParams) -> Result<Self, String> {
        let methods: Vec<String> = p
            .method
            .into_vec()
            .into_iter()
            .filter(|m| !m.is_empty())
            .collect();
        if methods.is_empty() {
            return Err(crate::t!(
                "'method' needs at least one method name.",
                "A 'method' legalább egy metódusnevet vár."
            )
            .into());
        }
        let mut query = Self::new(p.project, p.path, p.full);
        query.flags.push("--body".into());
        if p.raw {
            query.flags.push("--raw".into());
        } else if p.doc {
            query.flags.push("--doc".into());
        }
        query.methods = methods;
        Ok(query)
    }

    /// `find_callers`: `--grep <minta> --depth <n> [--guess]` a hatókörön.
    pub(crate) fn find_callers(p: FindCallersParams) -> Result<Self, String> {
        let depth = p.depth.unwrap_or(1);
        if !(1..=MAX_DEPTH).contains(&depth) {
            return Err(crate::tf!(
                "'depth' must be between 1 and {MAX_DEPTH} (got: {depth}).",
                "A 'depth' 1 és {MAX_DEPTH} között lehet (kaptam: {depth})."
            ));
        }
        let path = p.path.unwrap_or_default();
        let mut query = Self::new(p.project, path, p.full)
            .flag("--grep", non_empty(p.pattern)?)
            .flag("--depth", depth.to_string());
        if p.guess {
            query.flags.push("--guess".into());
        }
        if p.by_layer {
            query.flags.push("--by-layer".into());
        }
        Ok(query.layer(p.layer))
    }

    /// `search`: `--strings <minta>` vagy `--const <minta>`.
    pub(crate) fn search(p: SearchParams) -> Result<Self, String> {
        let flag = match p.kind {
            SearchKind::Strings => "--strings",
            SearchKind::Constants => "--const",
        };
        let path = p.path.unwrap_or_default();
        Ok(Self::new(p.project, path, p.full)
            .flag(flag, non_empty(p.pattern)?)
            .layer(p.layer))
    }

    /// `framework_view`: `--format <nézet>`.
    pub(crate) fn framework_view(p: FrameworkViewParams) -> Result<Self, String> {
        Ok(Self::new(p.project, p.path, p.full).flag("--format", p.view.as_str()))
    }

    /// `notes` (pilot): `--notes <minta>` (üres minta = minden jegyzet) a hatókörön; csak olvas.
    pub(crate) fn notes(p: NotesParams) -> Result<Self, String> {
        let path = p.path.unwrap_or_default();
        Ok(Self::new(p.project, path, false).flag("--notes", p.pattern.unwrap_or_default()))
    }

    /// `locate`: `--all` és a kifejezések (`--` után, hogy a `-`-sel kezdődő se legyen kapcsoló). A kifejezések
    /// nem utak, nem kell őket a gyökérre ellenőrizni.
    pub(crate) fn locate(p: LocateParams) -> Result<Self, String> {
        let terms: Vec<String> = p
            .terms
            .into_vec()
            .into_iter()
            .filter(|t| !t.trim().is_empty())
            .collect();
        if terms.is_empty() {
            return Err(crate::t!(
                "'terms' needs at least one name or term from the task.",
                "A 'terms' legalább egy nevet vagy fogalmat vár a feladatból."
            )
            .into());
        }
        let mut query = Self::new(p.project, String::new(), false);
        query.command = Subcommand::Locate;
        if p.all {
            query.flags.push("--all".into());
        }
        query.flags.push("--".into());
        query.flags.extend(terms);
        Ok(query)
    }

    /// A parancs eleje: a parancs neve és a feloldott gyökér (a `show`-nál a kimenet címkéje is).
    pub(crate) fn head(&self, root: &ProjectRoot) -> Vec<String> {
        let mut args = vec![
            match self.command {
                Subcommand::Show => "show",
                Subcommand::Locate => "locate",
            }
            .to_owned(),
            "--project".to_owned(),
            root.root().display().to_string(),
        ];
        if self.command == Subcommand::Show {
            args.extend(["--label".to_owned(), root.label().to_owned()]);
        }
        args
    }

    /// A `show` argumentumai (`show` és `--project`/`--label` nélkül): kapcsolók, majd a gyökéren belülre
    /// ellenőrzött cél(ok). A `locate`-nél csak a kapcsolók és a kifejezések.
    pub(crate) fn to_args(&self, root: &ProjectRoot) -> Result<Vec<String>, String> {
        if self.command == Subcommand::Locate {
            return Ok(self.flags.clone());
        }
        let target = root.resolve(&self.path)?;
        let mut args = self.flags.clone();
        if self.methods.is_empty() {
            args.push(target);
        } else {
            args.extend(self.methods.iter().map(|m| format!("{target}::{m}")));
        }
        Ok(args)
    }
}

fn non_empty(pattern: String) -> Result<String, String> {
    if pattern.is_empty() {
        return Err(crate::t!(
            "'pattern' must not be empty.",
            "A 'pattern' nem lehet üres."
        )
        .into());
    }
    Ok(pattern)
}

#[cfg(test)]
mod tests {
    use super::super::params::{FrameworkView, OneOrMany, OutlineFormat};
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    fn root() -> ProjectRoot {
        ProjectRoot::new(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
            "cg".into(),
        )
        .unwrap()
    }

    #[test]
    fn outline_maps_format_method_and_full() {
        let q = ShowQuery::outline(OutlineParams {
            path: "src/main.rs".into(),
            format: Some(OutlineFormat::Flow),
            method: Some("main".into()),
            full: true,
            ..OutlineParams::default()
        })
        .unwrap();
        let file = std::path::Path::new("src")
            .join("main.rs")
            .display()
            .to_string();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--full", "--format", "flow", &format!("{file}::main")])
        );
        let q = ShowQuery::outline(OutlineParams {
            path: "src".into(),
            ..OutlineParams::default()
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--format", "outline", "src"])
        );
    }

    #[test]
    fn method_body_selects_every_method() {
        let q = ShowQuery::method_body(MethodBodyParams {
            path: "Cargo.toml".into(),
            method: OneOrMany::Many(s(&["a", "", "b"])),
            ..MethodBodyParams::default()
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--body", "Cargo.toml::a", "Cargo.toml::b"])
        );
        assert!(ShowQuery::method_body(MethodBodyParams::default()).is_err());
        let q = ShowQuery::method_body(MethodBodyParams {
            path: "Cargo.toml".into(),
            method: OneOrMany::One("a".into()),
            doc: true,
            ..MethodBodyParams::default()
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--body", "--doc", "Cargo.toml::a"])
        );
        let q = ShowQuery::method_body(MethodBodyParams {
            path: "Cargo.toml".into(),
            method: OneOrMany::One("a".into()),
            raw: true,
            ..MethodBodyParams::default()
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--body", "--raw", "Cargo.toml::a"])
        );
    }

    #[test]
    fn find_callers_validates_depth_and_defaults_to_root() {
        let q = ShowQuery::find_callers(FindCallersParams {
            pattern: "Repo::find".into(),
            depth: Some(3),
            guess: true,
            ..FindCallersParams::default()
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--grep", "Repo::find", "--depth", "3", "--guess", "."])
        );
        for depth in [0, 6] {
            let p = FindCallersParams {
                pattern: "x".into(),
                depth: Some(depth),
                ..FindCallersParams::default()
            };
            assert!(ShowQuery::find_callers(p).is_err());
        }
        assert!(ShowQuery::find_callers(FindCallersParams::default()).is_err());
    }

    #[test]
    fn search_and_framework_views_map_to_flags() {
        let q = ShowQuery::search(SearchParams {
            kind: SearchKind::Constants,
            pattern: "STATUS".into(),
            path: Some("src".into()),
            project: Some("alias".into()),
            layer: None,
            full: false,
        })
        .unwrap();
        assert_eq!(q.project.as_deref(), Some("alias"));
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--const", "STATUS", "src"])
        );
        let q = ShowQuery::framework_view(FrameworkViewParams {
            view: FrameworkView::Routes,
            path: "src".into(),
            project: None,
            full: false,
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--format", "routes", "src"])
        );
    }

    #[test]
    fn layer_params_map_to_the_layer_flags() {
        let q = ShowQuery::outline(OutlineParams {
            path: "src".into(),
            layer: Some("service".into()),
            ..OutlineParams::default()
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--format", "outline", "--layer", "service", "src"])
        );
        let q = ShowQuery::find_callers(FindCallersParams {
            pattern: "Repo::find".into(),
            depth: Some(2),
            layer: Some("controller".into()),
            by_layer: true,
            ..FindCallersParams::default()
        })
        .unwrap();
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&[
                "--grep",
                "Repo::find",
                "--depth",
                "2",
                "--by-layer",
                "--layer",
                "controller",
                "."
            ])
        );
        let q = ShowQuery::search(SearchParams {
            kind: SearchKind::Strings,
            pattern: "x".into(),
            path: None,
            project: None,
            layer: Some(" ".into()),
            full: false,
        })
        .unwrap();
        assert_eq!(q.to_args(&root()).unwrap(), s(&["--strings", "x", "."]));
    }

    #[test]
    fn notes_maps_to_the_notes_flag() {
        let q = ShowQuery::notes(NotesParams {
            pattern: Some("cache".into()),
            path: Some("src".into()),
            project: None,
        })
        .unwrap();
        assert_eq!(q.to_args(&root()).unwrap(), s(&["--notes", "cache", "src"]));
        let q = ShowQuery::notes(NotesParams::default()).unwrap();
        assert_eq!(q.to_args(&root()).unwrap(), s(&["--notes", "", "."]));
    }

    #[test]
    fn locate_maps_terms_after_a_separator() {
        let q = ShowQuery::locate(LocateParams {
            terms: OneOrMany::Many(s(&["RefundController", " ", "-odd"])),
            project: Some("alias".into()),
            all: true,
        })
        .unwrap();
        assert_eq!(q.project.as_deref(), Some("alias"));
        assert_eq!(
            q.to_args(&root()).unwrap(),
            s(&["--all", "--", "RefundController", "-odd"])
        );
        let head = q.head(&root());
        assert_eq!(head[0], "locate");
        assert!(!head.contains(&"--label".to_owned()), "{head:?}");
        let show = ShowQuery::notes(NotesParams::default()).unwrap();
        assert_eq!(show.head(&root())[0], "show");
        assert!(show.head(&root()).contains(&"--label".to_owned()));
        assert!(ShowQuery::locate(LocateParams::default()).is_err());
    }

    #[test]
    fn escaping_targets_are_rejected() {
        let q = ShowQuery::outline(OutlineParams {
            path: "../etc".into(),
            ..OutlineParams::default()
        })
        .unwrap();
        assert!(q.to_args(&root()).is_err());
    }
}
