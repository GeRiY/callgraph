//! Fordításkori forrás-ujjlenyomat (`CALLGRAPH_SRC_HASH`): a cache-kulcs része, így minden új build
//! érvényteleníti a korábbi elemzéseket (nem kell kézzel léptetni a sémaverziót).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::{env, fs};

fn collect(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}

fn main() {
    let root = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let src = Path::new(&root).join("src");
    let mut files = Vec::new();
    collect(&src, &mut files);
    files.sort();
    let mut hasher = DefaultHasher::new();
    for file in &files {
        file.strip_prefix(&src).unwrap_or(file).hash(&mut hasher);
        fs::read(file).unwrap_or_default().hash(&mut hasher);
    }
    println!(
        "cargo:rustc-env=CALLGRAPH_SRC_HASH={:016x}",
        hasher.finish()
    );
    println!("cargo:rerun-if-changed=src");
}
