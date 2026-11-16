//! Factory: a megfelelő stratégia kiválasztása a projekt composer-függőségei alapján (a legközelebbi
//! `composer.json`, a csomagtérkép olvasójával: `crate::packages`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::{FrameworkStrategy, GenericStrategy, LaravelStrategy, YiiStrategy};

type StrategyFactory = fn() -> Rc<dyn FrameworkStrategy>;

const FRAMEWORK_PACKAGE_MAP: &[(&str, StrategyFactory)] = &[
    ("laravel/framework", || Rc::new(LaravelStrategy)),
    ("yiisoft/yii2", || Rc::new(YiiStrategy)),
];

/// Könyvtáranként cache-elt stratégia-választó.
#[derive(Default)]
pub struct FrameworkDetector {
    cache: HashMap<PathBuf, Rc<dyn FrameworkStrategy>>,
}

impl FrameworkDetector {
    pub fn new() -> Self {
        Self::default()
    }

    /// A könyvtárhoz tartozó stratégia; az eredmény könyvtáranként cache-elt.
    pub fn detect(&mut self, dir: &Path) -> Rc<dyn FrameworkStrategy> {
        let strategy = self.cache.entry(dir.to_path_buf()).or_insert_with(|| {
            let framework = find_composer_json(dir)
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|json| detect_framework(&json));
            framework
                .and_then(|name| {
                    FRAMEWORK_PACKAGE_MAP
                        .iter()
                        .find(|(_, factory)| factory().name() == Some(name.as_str()))
                        .map(|(_, factory)| factory())
                })
                .unwrap_or_else(|| Rc::new(GenericStrategy))
        });
        Rc::clone(strategy)
    }
}

fn find_composer_json(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir;
    loop {
        let candidate = dir.join("composer.json");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// A `composer.json` frameworkje: előbb a `require`, utána a `require-dev` csomagjai közül az első, amely a
/// `FRAMEWORK_PACKAGE_MAP`-ben szerepel (a map sorrendjében). Az olvasás a csomagtérképével közös
/// (`packages::composer_requires`).
fn detect_framework(json: &str) -> Option<String> {
    let requires = crate::packages::composer_requires(json);
    [false, true].into_iter().find_map(|dev| {
        FRAMEWORK_PACKAGE_MAP
            .iter()
            .find(|(package, _)| {
                requires
                    .iter()
                    .any(|(name, is_dev)| *is_dev == dev && name == package)
            })
            .map(|(_, factory)| factory().name().unwrap().to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_composer_is_generic() {
        assert_eq!(detect_framework("not json"), None);
    }

    #[test]
    fn composer_requires_detect_framework() {
        assert_eq!(
            detect_framework(r#"{"require":{"laravel/framework":"^11"}}"#),
            Some("laravel".into())
        );
    }
}
