//! Beágyazott framework-másolat felismerése (a `vendor.rs` része): egy régi projektbe bemásolt framework-mag
//! (CodeIgniter `system/`, Yii `framework/`, Kohana `system/`) saját `Commands`, `Handlers`, `Request` mappái ne
//! adjanak rétegeket a projekt profiljába. Csak a framework-mag mappája marad ki (az egység maga), a mellette lévő
//! alkalmazás-kód (`application/`, `protected/`, `app/`) nem: az a projekt saját kódja.
//!
//! A szabály egy-egy árulkodó fájl a mappában (nem a mappanév): ezek a framework belépő osztályai, saját kódban nem
//! fordulnak elő ezen a helyen. A gyökér maga soha nem másolat, és ha a gyökér `composer.json`-ja maga a framework
//! (`codeigniter4/…`, `yiisoft/…`, `kohana/…` vendor-előtag), a szabály nem él — a framework saját repójában a mag
//! a projekt kódja.

use std::path::Path;

/// A framework-mag árulkodó fájljai (a mappához relatív út; bármelyik elég).
const MARKERS: &[&str] = &[
    // CodeIgniter 4 `system/`, CodeIgniter 2–3 `system/`
    "CodeIgniter.php",
    "core/CodeIgniter.php",
    // Yii 1 `framework/`, Yii 2 bemásolt `framework/`
    "YiiBase.php",
    "BaseYii.php",
    // Kohana `system/`
    "classes/Kohana/Core.php",
    "classes/kohana/core.php",
];

/// A framework-ök vendor-előtagjai (kisbetűvel): ha a gyökér maga ilyen csomag, a mag nem másolat.
pub(crate) const FRAMEWORK_VENDORS: &[&str] = &["codeigniter", "codeigniter4", "yiisoft", "kohana"];

/// A mappa egy bemásolt framework-mag-e (egy árulkodó fájl alapján).
pub(crate) fn is_framework_core(dir: &Path) -> bool {
    MARKERS.iter().any(|marker| dir.join(marker).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_identify_the_framework_core_only() {
        let root = std::env::temp_dir().join(format!(
            "callgraph-framework-copy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let write = |rel: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "<?php\n").unwrap();
        };
        write("legacy/system/CodeIgniter.php");
        write("legacy/app/Controllers/Home.php");
        write("old/framework/YiiBase.php");
        write("k/system/classes/Kohana/Core.php");
        write("own/system/Settings.php");
        assert!(is_framework_core(&root.join("legacy/system")));
        assert!(is_framework_core(&root.join("old/framework")));
        assert!(is_framework_core(&root.join("k/system")));
        assert!(!is_framework_core(&root.join("legacy")));
        assert!(!is_framework_core(&root.join("legacy/app")));
        assert!(!is_framework_core(&root.join("own/system")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
