use std::path::PathBuf;

use super::*;

fn home_with(lang_file: Option<&str>) -> (SkillHome, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "callgraph-i18n-{}-{}-{}",
        std::process::id(),
        {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        },
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(content) = lang_file {
        std::fs::write(dir.join("lang"), content).unwrap();
    }
    let home = SkillHome::new(
        dir.clone(),
        dir.display().to_string(),
        dir.join("projects.json"),
    );
    (home, dir)
}

#[test]
fn parses_codes_and_locales() {
    assert_eq!(Lang::parse("en"), Some(Lang::En));
    assert_eq!(Lang::parse(" HU\n"), Some(Lang::Hu));
    assert_eq!(Lang::parse("hu_HU.UTF-8"), Some(Lang::Hu));
    assert_eq!(Lang::parse("en-US"), Some(Lang::En));
    assert_eq!(Lang::parse("de"), None);
    assert_eq!(Lang::parse(""), None);
}

#[test]
fn default_is_english() {
    let (home, dir) = home_with(None);
    assert_eq!(resolve(None, &home), (Lang::En, LangSource::Default));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn file_overrides_default_and_env_overrides_file() {
    let (home, dir) = home_with(Some("hu\n"));
    assert_eq!(resolve(None, &home), (Lang::Hu, LangSource::File));
    assert_eq!(
        resolve(Some("en".into()), &home),
        (Lang::En, LangSource::Env)
    );
    // érvénytelen környezeti érték: a fájl dönt
    assert_eq!(
        resolve(Some("xx".into()), &home),
        (Lang::Hu, LangSource::File)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pick_selects_by_language() {
    assert_eq!(Lang::En.pick("Usage", "Használat"), "Usage");
    assert_eq!(Lang::Hu.pick("Usage", "Használat"), "Használat");
}
