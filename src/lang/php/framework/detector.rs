//! Factory: a megfelelő stratégia kiválasztása a projekt composer-függőségei alapján.

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

fn detect_framework(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    ["require", "require-dev"].iter().find_map(|section| {
        let packages = value.get(*section)?.as_object()?;
        FRAMEWORK_PACKAGE_MAP
            .iter()
            .find(|(package, _)| packages.contains_key(*package))
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
