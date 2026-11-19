//! Filament: resources with their model, pages, form fields, table columns, actions and relation managers, the
//! standalone pages / widgets / panels, the names checked against the model, the `filament` view, `trace` and the
//! graph key. Synthetic project: `tests/fixtures/web/filament_app` (a panel, two resources, a wrong field name, a wrong
//! relation manager, a nested section schema, a custom action, a widget, a page, a model with its migration).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// A temporary copy of a sample project (the cache goes into the copy, never into the repo).
struct Fixture(PathBuf);

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn temp_root() -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "callgraph-filament-{id}-{}-{n}",
        std::process::id()
    ))
}

impl Fixture {
    fn sample() -> Self {
        let root = temp_root();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/web/filament_app");
        copy_dir(&source, &root);
        Self(root.canonicalize().unwrap())
    }

    /// A project written from `(path, text)` pairs.
    fn files(files: &[(&str, &str)]) -> Self {
        let root = temp_root();
        for (path, text) in files {
            let target = root.join(path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, text).unwrap();
        }
        Self(root.canonicalize().unwrap())
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_callgraph"))
            .args(args)
            .arg("--project")
            .arg(&self.0)
            .current_dir(&self.0)
            .env("CALLGRAPH_LANG", "en")
            .env("CALLGRAPH_UPDATE_CHECK", "0")
            .env("CALLGRAPH_REGISTRY", "/nonexistent/projects.json")
            .env_remove("CALLGRAPH_NO_GRAPH")
            .output()
            .expect("callgraph runs");
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fi_view_has_one_block_per_resource() {
    let out = Fixture::sample().run(&["show", "app", "--format", "filament"]);
    assert!(
        out.starts_with("filament: resources 2, pages 1, widgets 1, panels 1, warnings 2\n"),
        "{out}"
    );
    assert!(
        out.contains("PostResource  app/Filament/Resources/PostResource.php → Post(posts)\n"),
        "{out}"
    );
    assert!(
        out.contains("  pages: index /admin/posts · create · edit /{record}/edit\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "  form: title*, slug*, body, tilte, reading_time, author_id* (author), published_at, meta\n"
        ),
        "{out}"
    );
    assert!(
        out.contains("  table: title, author.name, status, reading_time, comments_count, published_at · filters: status · actions: edit, delete, publish · bulk: delete\n"),
        "{out}"
    );
    assert!(
        out.contains("  relations: CommentsRelationManager, ReviewsRelationManager\n"),
        "{out}"
    );
    // the slug override
    assert!(
        out.contains("UserResource  app/Filament/Resources/UserResource.php → User(users)\n"),
        "{out}"
    );
    assert!(
        out.contains("  pages: index /admin/team · edit /{record}/edit\n"),
        "{out}"
    );
    assert!(out.contains("  form: name*, email*\n"), "{out}");
    assert!(out.contains("  table: name, email, posts.title\n"), "{out}");
    // the standalone page, the widget (with the models it queries) and the panel
    assert!(
        out.contains("page Settings  app/Filament/Pages/Settings.php · /admin/settings"),
        "{out}"
    );
    assert!(
        out.contains("widget StatsOverview (stats)  app/Filament/Widgets/StatsOverview.php · models: Post(posts), Comment(comments)"),
        "{out}"
    );
    assert!(
        out.contains("panel admin /admin  app/Providers/Filament/AdminPanelProvider.php · resources: PostResource, UserResource"),
        "{out}"
    );
}

#[test]
fn fi_findings_only_for_the_wrong_field_and_the_wrong_relation() {
    let out = Fixture::sample().run(&["show", "app", "--format", "filament"]);
    assert!(
        out.contains("  ⚠ form field 'tilte' is not a column of Post(posts)  app/Filament/Resources/PostResource.php L41\n"),
        "{out}"
    );
    assert!(
        out.contains("  ⚠ ReviewsRelationManager: Post(posts) has no relation 'reviews'  app/Filament/Resources/PostResource/RelationManagers/ReviewsRelationManager.php L7\n"),
        "{out}"
    );
    assert_eq!(out.matches('⚠').count(), 2, "{out}");
    // the dotted relation column, the accessor, the aggregate, the JSON repeater and the `dehydrated(false)` field
    for quiet in [
        "'author.name'",
        "'reading_time'",
        "'comments_count'",
        "'posts.title'",
        "'meta'",
        "'key'",
    ] {
        assert!(!out.contains(quiet), "{quiet}: {out}");
    }
}

#[test]
fn fi_names_built_at_run_time_go_to_the_footer() {
    let out = Fixture::sample().run(&["show", "app", "--format", "filament"]);
    assert!(
        out.contains(
            "? 1 skipped: form field name is a run-time value (app/Filament/Resources/UserResource.php L26)"
        ),
        "{out}"
    );
}

#[test]
fn fi_no_column_findings_without_a_migration() {
    let fx = Fixture::files(&[
        (
            "composer.json",
            r#"{"require": {"laravel/framework": "^11.0", "filament/filament": "^3.2"}}"#,
        ),
        (
            "app/Models/Post.php",
            "<?php\nnamespace App\\Models;\nuse Illuminate\\Database\\Eloquent\\Model;\nclass Post extends Model\n{\n    protected $fillable = ['title'];\n}\n",
        ),
        (
            "app/Filament/Resources/PostResource.php",
            "<?php\nnamespace App\\Filament\\Resources;\nuse App\\Models\\Post;\nuse Filament\\Forms\\Components\\TextInput;\nuse Filament\\Forms\\Form;\nuse Filament\\Resources\\Resource;\nclass PostResource extends Resource\n{\n    protected static ?string $model = Post::class;\n    public static function form(Form $form): Form\n    {\n        return $form->schema([TextInput::make('title'), TextInput::make('nonsense')]);\n    }\n}\n",
        ),
    ]);
    let out = fx.run(&["show", "app", "--format", "filament"]);
    assert!(out.contains("PostResource"), "{out}");
    assert!(out.contains("form: title, nonsense"), "{out}");
    assert!(!out.contains('⚠'), "{out}");
}

#[test]
fn fi_trace_of_a_resource_by_slug_class_and_model() {
    let fx = Fixture::sample();
    for target in ["filament:posts", "PostResource", "filament:Post"] {
        let out = fx.run(&["trace", target]);
        assert!(
            out.starts_with("filament PostResource  app/Filament/Resources/PostResource.php:"),
            "{target}: {out}"
        );
        assert!(
            out.contains("model Post(posts)  app/Models/Post.php\n"),
            "{out}"
        );
        assert!(
            out.contains("panel admin /admin  app/Providers/Filament/AdminPanelProvider.php:"),
            "{out}"
        );
        assert!(
            out.contains("pages index /admin/posts (ListPosts), create /admin/posts/create (CreatePost), edit /admin/posts/{record}/edit (EditPost)\n"),
            "{out}"
        );
        assert!(out.contains("form title*, slug*, body,"), "{out}");
        assert!(
            out.contains(
                "relations CommentsRelationManager → comments, ReviewsRelationManager → reviews\n"
            ),
            "{out}"
        );
        assert!(
            out.contains("⚠ form field 'tilte' is not a column of Post(posts)"),
            "{out}"
        );
    }
    let team = fx.run(&["trace", "filament:team"]);
    assert!(team.contains("pages index /admin/team"), "{team}");
}

#[test]
fn fi_resource_key_finds_the_resource_from_the_model_side() {
    let out = Fixture::sample().run(&["show", "app", "--grep", "filament::resource:Post"]);
    assert!(
        out.contains("app/Filament/Resources/PostResource.php"),
        "{out}"
    );
    assert!(!out.contains("UserResource"), "{out}");
}
