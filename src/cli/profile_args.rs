//! A projekt-profil kapcsolói: a `profile` parancs (`callgraph profile [--project <alias|mappa>] [--full]
//! [--with-vendor]`), és a
//! `show` réteg-kapcsolói (`--layer <név,…>`, `--layers`, `--by-layer`). Külön fájlban, hogy az `args.rs` csak
//! egy-egy sorral bővüljön.

use crate::error::{CallgraphError, Result};

use super::CliArgs;

fn profile_usage() -> &'static str {
    crate::t!(
        "callgraph profile [--project <alias|dir>] [--full] [--with-vendor]  (architecture, layers and patterns from folder and file names, checked against the graph; --with-vendor: embedded third-party code counts too)",
        "callgraph profile [--project <alias|mappa>] [--full] [--with-vendor]  (architektúra, rétegek és minták a mappa- és fájlnevekből, a gráffal ellenőrizve; --with-vendor: a beágyazott külső kód is számít)"
    )
}

/// A `show` réteg-kapcsolói; mind opcionális, alapból kikapcsolva (a kimenet ilyenkor változatlan).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerArgs {
    /// `--layer service[,repository]`: csak e rétegek fájljai (a mappa-bejárásban).
    pub layer: Option<String>,
    /// `--layers`: rétegenként csoportosított summary, és a szerep a `FILE` sorok végén.
    pub layers: bool,
    /// `--by-layer`: a „ki hívja?” hívói rétegek szerint, a hívotthoz legközelebbi réteg elöl.
    pub by_layer: bool,
}

impl LayerArgs {
    /// Kért-e bármelyik réteg-viselkedést.
    pub fn any(&self) -> bool {
        self.layer.is_some() || self.layers || self.by_layer
    }

    /// Ha `args[0]` egy réteg-kapcsoló: feldolgozza, és visszaadja, hány argumentumot fogyasztott.
    pub(crate) fn take(&mut self, args: &[String]) -> Result<Option<usize>> {
        let Some(first) = args.first() else {
            return Ok(None);
        };
        Ok(match first.as_str() {
            "--layers" => {
                self.layers = true;
                Some(1)
            }
            "--by-layer" => {
                self.by_layer = true;
                Some(1)
            }
            "--layer" => {
                let value = args.get(1).filter(|v| !v.starts_with('-')).ok_or_else(|| {
                    CallgraphError::Usage(
                        crate::t!(
                            "--layer needs a layer name (e.g. --layer service; list: callgraph profile --full)",
                            "A --layer egy rétegnevet vár (pl. --layer service; lista: callgraph profile --full)"
                        )
                        .into(),
                    )
                })?;
                self.layer = Some(value.clone());
                Some(2)
            }
            value if value.starts_with("--layer=") => {
                self.layer = Some(value["--layer=".len()..].to_owned());
                Some(1)
            }
            _ => None,
        })
    }
}

/// `profile [--project <alias|mappa>] [--full] [--with-vendor]`.
pub(crate) fn parse_profile(args: &[String]) -> Result<CliArgs> {
    let mut project = None;
    let mut full = false;
    let mut with_vendor = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--full" => {
                full = true;
                i += 1;
            }
            "--with-vendor" => {
                with_vendor = true;
                i += 1;
            }
            "--project" if i + 1 < args.len() => {
                project = Some(args[i + 1].clone());
                i += 2;
            }
            value if value.starts_with("--project=") => {
                project = Some(value["--project=".len()..].to_owned());
                i += 1;
            }
            _ => return Err(CallgraphError::Usage(profile_usage().into())),
        }
    }
    Ok(CliArgs::Profile {
        project,
        full,
        with_vendor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn layer_flags_are_consumed_one_by_one() {
        let mut layers = LayerArgs::default();
        assert!(!layers.any());
        assert_eq!(
            layers.take(&s(&["--layer", "service", "app"])).unwrap(),
            Some(2)
        );
        assert_eq!(layers.take(&s(&["--layers"])).unwrap(), Some(1));
        assert_eq!(layers.take(&s(&["--by-layer"])).unwrap(), Some(1));
        assert_eq!(layers.take(&s(&["app"])).unwrap(), None);
        assert_eq!(layers.take(&s(&["--full"])).unwrap(), None);
        assert_eq!(
            layers,
            LayerArgs {
                layer: Some("service".into()),
                layers: true,
                by_layer: true
            }
        );
        assert!(layers.take(&s(&["--layer"])).is_err());
        assert!(layers.take(&s(&["--layer", "--full"])).is_err());
        assert_eq!(layers.take(&s(&["--layer=model"])).unwrap(), Some(1));
        assert_eq!(layers.layer.as_deref(), Some("model"));
    }

    #[test]
    fn parses_profile() {
        assert_eq!(
            parse_profile(&s(&["--project", "sp", "--full"])).unwrap(),
            CliArgs::Profile {
                project: Some("sp".into()),
                full: true,
                with_vendor: false
            }
        );
        assert_eq!(
            parse_profile(&[]).unwrap(),
            CliArgs::Profile {
                project: None,
                full: false,
                with_vendor: false
            }
        );
        assert_eq!(
            parse_profile(&s(&["--with-vendor"])).unwrap(),
            CliArgs::Profile {
                project: None,
                full: false,
                with_vendor: true
            }
        );
        assert!(parse_profile(&s(&["--bogus"])).is_err());
    }
}
