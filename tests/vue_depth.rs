//! `--grep <komponens>.vue --depth N` (valódi binárissal): minden szinten a template-használók a következő szint —
//! molekula → az azt használó organizmus (1. szint) → az organizmust használó oldal (2. szint).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const MOLECULE: &str = "<template>
  <span class=\"chip\"><slot /></span>
</template>
";

const ORGANISM: &str = "<script setup>
import MoleculeX from '../Molecules/MoleculeX.vue'
</script>

<template>
  <div class=\"card\">
    <MoleculeX>tag</MoleculeX>
  </div>
</template>
";

const PAGE: &str = "<script setup>
import OrganismY from '../Organisms/OrganismY.vue'
</script>

<template>
  <main>
    <OrganismY />
  </main>
</template>
";

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "callgraph-vue-depth-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let js = root.join("resources/js");
        for dir in ["Molecules", "Organisms", "Pages"] {
            fs::create_dir_all(js.join(dir)).unwrap();
        }
        let project = Self(root.canonicalize().unwrap());
        let js = project.0.join("resources/js");
        fs::write(js.join("Molecules/MoleculeX.vue"), MOLECULE).unwrap();
        fs::write(js.join("Organisms/OrganismY.vue"), ORGANISM).unwrap();
        fs::write(js.join("Pages/Home.vue"), PAGE).unwrap();
        fs::write(project.0.join("package.json"), "{\"name\":\"x\"}\n").unwrap();
        project
    }

    fn run(&self, args: &[&str], graph: bool) -> String {
        let mut command = Command::new(env!("CARGO_BIN_EXE_callgraph"));
        command
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH");
        if !graph {
            command.env("CALLGRAPH_NO_GRAPH", "1");
        }
        let out = command.output().expect("callgraph runs");
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().replace('\\', "/")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn depth_follows_template_usage_beyond_the_first_level() {
    let project = Project::new();
    let out = project.run(
        &[
            "resources/js",
            "--grep",
            "Molecules/MoleculeX.vue",
            "--depth",
            "3",
        ],
        true,
    );
    let organism = out.find("OrganismY").expect(&out);
    let page = out.find("Home").expect(&out);
    assert!(organism < page, "{out}");
    assert!(!out.contains("no further callers"), "{out}");
}
