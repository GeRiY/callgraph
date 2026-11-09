//! A `.gitignore` és a `.callgraphignore` a teljes láncon (bejárás → `locate` / mappa-nézet): egy Laravel + Vite
//! projekt `public/build` kimenete sosem jelenik meg; `--no-gitignore`-ral a `.gitignore` nem számít, de a
//! minifikált build-fájl akkor is kimarad; a `.callgraphignore` útmintája `.gitignore` nélkül is hat, a
//! kiterjesztés-sora a régi módon.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-ignore-it-{}-{id}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// A Laravel + Vite projekt: egy minifikált (egysoros) és egy nem minifikált build-fájl, egy Vue-komponens.
    fn vite() -> Self {
        let project = Self::new();
        let bundle = format!(
            "import{{a as e}}from\"./vendor.js\";const EventDetailContent={{name:\"EventDetailContent\",setup(){{return{{t:\"{}\"}}}}}};export{{EventDetailContent as default}};\n",
            "x".repeat(4000)
        );
        project.write("public/build/assets/app-CBW7lKWe.js", &bundle);
        project.write(
            "public/build/assets/plain-Dk3x9.js",
            "export function EventDetailContentLoader() {\n  return 'EventDetailContent';\n}\n",
        );
        project.write(
            "resources/js/Components/EventDetailContent.vue",
            "<template>\n  <div>{{ title }}</div>\n</template>\n\n<script setup>\nconst title = 'EventDetailContent';\n</script>\n",
        );
        project
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .env_remove("CALLGRAPH_GITIGNORE")
            .output()
            .expect("callgraph runs");
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().replace('\\', "/")
    }

    fn locate(&self, extra: &[&str]) -> String {
        let root = self.0.to_string_lossy().into_owned();
        let mut args = vec!["locate", "EventDetailContent", "--all", "--project"];
        args.push(&root);
        args.extend_from_slice(extra);
        self.run(&args)
    }

    fn folder(&self, extra: &[&str]) -> String {
        let root = self.0.to_string_lossy().into_owned();
        let mut args = vec!["show", "--project"];
        args.push(&root);
        args.extend_from_slice(extra);
        args.push(".");
        self.run(&args)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const BUNDLE: &str = "app-CBW7lKWe.js";
const PLAIN: &str = "plain-Dk3x9.js";
const COMPONENT: &str = "resources/js/Components/EventDetailContent.vue";

#[test]
fn gitignored_build_output_is_never_listed() {
    let project = Project::vite();
    project.write(".gitignore", "/node_modules\n/public/build\n/public/hot\n");
    for out in [project.locate(&[]), project.folder(&[])] {
        assert!(out.contains(COMPONENT), "{out}");
        assert!(!out.contains(BUNDLE) && !out.contains(PLAIN), "{out}");
    }
    // kikapcsolt .gitignore: a nem minifikált build-fájl látszik, a minifikált (egysoros) továbbra sem
    for out in [
        project.locate(&["--no-gitignore"]),
        project.folder(&["--no-gitignore"]),
    ] {
        assert!(out.contains(COMPONENT), "{out}");
        assert!(out.contains(PLAIN), "{out}");
        assert!(!out.contains(BUNDLE), "{out}");
    }
    // ugyanaz a környezeti változóval (MCP / hook), és a következő normál futás újra kihagyja (a gráf nem keveri)
    let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
        .args(["locate", "EventDetailContent", "--all", "--project"])
        .arg(&project.0)
        .env("CALLGRAPH_LANG", "en")
        .env("CALLGRAPH_UPDATE_CHECK", "0")
        .env("CALLGRAPH_STATS", "0")
        .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
        .env("CALLGRAPH_GITIGNORE", "0")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains(PLAIN));
    let out = project.locate(&[]);
    assert!(!out.contains(PLAIN) && out.contains(COMPONENT), "{out}");
}

#[test]
fn a_minified_bundle_is_skipped_without_any_ignore_file() {
    let project = Project::vite();
    let out = project.locate(&[]);
    assert!(out.contains(COMPONENT) && out.contains(PLAIN), "{out}");
    assert!(!out.contains(BUNDLE), "{out}");
    let folder = project.folder(&[]);
    assert!(!folder.contains(BUNDLE), "{folder}");
}

#[test]
fn callgraphignore_path_patterns_work_without_gitignore() {
    for pattern in ["public/build", "/public/build/", "public/build/**"] {
        let project = Project::vite();
        project.write(".callgraphignore", &format!("# build\n{pattern}\n"));
        for extra in [&[][..], &["--no-gitignore"][..]] {
            let out = project.locate(extra);
            assert!(out.contains(COMPONENT), "{pattern}: {out}");
            assert!(!out.contains(PLAIN), "{pattern} {extra:?}: {out}");
        }
    }
    // a kiterjesztés-sor a régi módon: a .vue kimarad, a build-fájl marad
    let project = Project::vite();
    project.write(".callgraphignore", "vue\n");
    let out = project.locate(&[]);
    assert!(!out.contains(COMPONENT) && out.contains(PLAIN), "{out}");
}
