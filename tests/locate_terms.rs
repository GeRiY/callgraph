//! `callgraph locate` különleges kifejezései: `route::név` (a definíció elöl, a PHP- és JS-hívások), a több szavas
//! kifejezés (egészében a stringekre), a `--név` CSS custom property; a kimenet mindig `root:` sorral és relatív
//! utakkal.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct Project(PathBuf);

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "callgraph-locate-terms-{}-{id}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let project = Self(root.canonicalize().unwrap());
        for (rel, text) in files {
            let path = project.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        project
    }

    fn locate(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .arg("locate")
            // a `--project` elöl: a `--` után minden argumentum kifejezés
            .arg("--project")
            .arg(&self.0)
            .args(args)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_STATS", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .output()
            .expect("callgraph runs");
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

/// A rangsor fájl-sorai (`1. út  ★★★`).
fn ranked(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| line.split_once(". "))
        .filter(|(n, _)| n.parse::<usize>().is_ok())
        .map(|(_, rest)| rest.split("  ").next().unwrap_or(rest).to_owned())
        .collect()
}

/// A gyökér sora elöl, egyszer; utána minden út relatív.
fn assert_root_line(project: &Project, out: &str) {
    assert!(out.starts_with("root: "), "{out}");
    let root = project.0.display().to_string().replace('\\', "/");
    let root = root.strip_prefix("//?/").unwrap_or(&root).to_owned();
    assert_eq!(out.matches(&root).count(), 1, "{out}");
}

fn laravel() -> Project {
    Project::new(&[
        (
            "routes/web.php",
            "<?php\nuse App\\Http\\Controllers\\EventController;\nuse Illuminate\\Support\\Facades\\Route;\n\nRoute::get('/events/{e}', [EventController::class, 'show'])->name('events.show');\nRoute::get('/events', [EventController::class, 'index'])->name('events.index');\n",
        ),
        (
            "app/Http/Controllers/EventController.php",
            "<?php\nnamespace App\\Http\\Controllers;\n\nclass EventController\n{\n    public function store()\n    {\n        return redirect()->route('events.show', ['e' => 1]);\n    }\n}\n",
        ),
        (
            "app/Http/Resources/EventResource.php",
            "<?php\nnamespace App\\Http\\Resources;\n\nclass EventResource\n{\n    public function toArray($request)\n    {\n        return ['url' => route('events.show', [$this->id])];\n    }\n}\n",
        ),
        (
            "resources/js/Composables/useNavigation.js",
            "export function goEvent(id) {\n  return route('events.show', id);\n}\n",
        ),
        (
            "app/Models/Event.php",
            "<?php\nnamespace App\\Models;\n\nclass Event\n{\n    public function url()\n    {\n        return to_route('events.index');\n    }\n}\n",
        ),
    ])
}

#[test]
fn a_route_name_finds_its_definition_first_and_every_call() {
    let project = laravel();
    let out = project.locate(&["route::events.show", "--all"]);
    let files = ranked(&out);
    assert_eq!(files.len(), 4, "{out}");
    assert_eq!(files[0], "routes/web.php", "{out}");
    for file in [
        "app/Http/Controllers/EventController.php",
        "app/Http/Resources/EventResource.php",
        "resources/js/Composables/useNavigation.js",
    ] {
        assert!(files.contains(&file.to_owned()), "{file}: {out}");
    }
    assert!(out.contains("(route definition)"), "{out}");
    assert!(!out.contains("Event.php  "), "{out}");
    assert_root_line(&project, &out);
}

#[test]
fn an_unknown_route_name_says_what_was_searched() {
    let project = laravel();
    let out = project.locate(&["route::events.nothing"]);
    assert!(out.contains("no match: route::events.nothing"), "{out}");
    assert!(out.contains("->name('events.nothing')"), "{out}");
    assert!(ranked(&out).is_empty(), "{out}");
}

#[test]
fn a_quoted_phrase_is_one_term_matched_as_a_whole() {
    let project = Project::new(&[
        (
            "resources/js/Components/EventDetailContent.vue",
            "<template>\n  <h2>{{ $t('Szervező további eseményei') }}</h2>\n</template>\n\n<script setup>\nconst props = defineProps({ event: Object });\n</script>\n",
        ),
        (
            "resources/js/Pages/MyEvents.vue",
            "<template>\n  <h1>{{ $t('Eseményeim') }}</h1>\n</template>\n\n<script setup>\nconst title = 'Eseményeim';\n</script>\n",
        ),
        (
            "lang/hu.php",
            "<?php\nreturn [\n    'organizer_more' => 'Szervező további eseményei',\n    'event_detail' => 'EventDetailContent',\n    'mine' => 'Eseményeim',\n];\n",
        ),
    ]);
    let out = project.locate(&["Szervező további eseményei"]);
    assert!(out.contains("locate: 1 terms · "), "{out}");
    let files = ranked(&out);
    assert!(files.contains(&"lang/hu.php".to_owned()), "{out}");
    // a `eseményei` szó nem illeszt név-részletként az `Eseményeim`-re
    let mine = "resources/js/Pages/MyEvents.vue".to_owned();
    assert!(!files.contains(&mine), "{out}");
    // több kifejezésnél a pontos komponens-definíció elöl, a több kifejezésre stringgel illő fordítási fájl előtt
    let out = project.locate(&["Szervező további eseményei", "EventDetailContent"]);
    assert_eq!(
        ranked(&out).first().map(String::as_str),
        Some("resources/js/Components/EventDetailContent.vue"),
        "{out}"
    );
}

#[test]
fn a_css_custom_property_is_a_term_not_a_flag() {
    let project = Project::new(&[(
        "resources/css/app.css",
        ":root {\n  --orange-500: #f97316;\n}\n\n.btn {\n  color: var(--orange-500);\n}\n",
    )]);
    for args in [&["--orange-500"][..], &["--", "--orange-500"][..]] {
        let out = project.locate(args);
        assert!(out.contains("1. resources/css/app.css"), "{args:?}: {out}");
        assert!(out.contains("--orange-500"), "{out}");
        assert_root_line(&project, &out);
    }
}

#[test]
fn an_identifier_joining_several_terms_ranks_first_even_for_general_terms() {
    // a koel-eset kicsiben: a `toggleFavorite` és a `PodcastEpisodeController` több kifejezést egyesít; a
    // típusdeklaráció (`.d.ts`) és az egy-egy kifejezést definícióval / szövegben tartalmazó fájlok hátrébb
    let mut files: Vec<(String, String)> = vec![
        (
            "app/Services/FavoriteService.php".into(),
            "<?php\nnamespace App\\Services;\n\nclass FavoriteService\n{\n    public function toggleFavorite(int $id): bool\n    {\n        return true;\n    }\n}\n".into(),
        ),
        (
            "app/Http/Controllers/PodcastEpisodeController.php".into(),
            "<?php\nnamespace App\\Http\\Controllers;\n\nclass PodcastEpisodeController\n{\n    public function index(): array\n    {\n        return [];\n    }\n}\n".into(),
        ),
        (
            "app/Models/Podcast.php".into(),
            "<?php\nnamespace App\\Models;\n\nclass Podcast\n{\n    public function episodes(): array\n    {\n        return ['favorite'];\n    }\n}\n".into(),
        ),
        (
            "resources/js/types.d.ts".into(),
            "interface Favorite {\n  id: number\n}\n\ninterface Podcast {\n  title: string\n}\n\ninterface Episode {\n  title: string\n}\n".into(),
        ),
        (
            "resources/js/services/queue.ts".into(),
            "export function toggle(): string {\n  return 'episode_image' + 'podcast_author'\n}\n".into(),
        ),
        (
            "resources/js/components/ActionMenu.vue".into(),
            "<template>\n  <button @click=\"toggle\">x</button>\n</template>\n\n<script setup lang=\"ts\">\nfunction toggle() {\n  return 'favorite' + 'podcast_author'\n}\n</script>\n".into(),
        ),
    ];
    // sok fájl a szövegben: mind a négy kifejezés túl általános
    for i in 0..31 {
        files.push((
            format!("app/Lists/List{i}.php"),
            format!("<?php\nfunction list{i}()\n{{\n    return 'favorite toggle podcast episode';\n}}\n"),
        ));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(rel, text)| (rel.as_str(), text.as_str()))
        .collect();
    let project = Project::new(&refs);
    let out = project.locate(&["favorite", "podcast", "episode", "toggle", "--all"]);
    let files = ranked(&out);
    let at = |path: &str| {
        files
            .iter()
            .position(|p| p == path)
            .unwrap_or_else(|| panic!("{path}: {out}"))
    };
    let favorite = at("app/Services/FavoriteService.php");
    let episodes = at("app/Http/Controllers/PodcastEpisodeController.php");
    assert!(favorite < 2 && episodes < 2, "{out}");
    for later in [
        "resources/js/types.d.ts",
        "resources/js/services/queue.ts",
        "resources/js/components/ActionMenu.vue",
        "app/Models/Podcast.php",
        "app/Lists/List0.php",
    ] {
        assert!(at(later) > favorite.max(episodes), "{later}: {out}");
    }
    assert!(out.contains("toggleFavorite()"), "{out}");
    // a túl általános jelzés megmarad
    assert!(
        out.contains("too general (matched in more than 30 files): favorite, podcast, episode, toggle"),
        "{out}"
    );
}
