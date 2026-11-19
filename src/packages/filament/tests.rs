//! Unit tests of the Filament module: the scanners on the synthetic fixture files and on small snippets, the checks
//! against a model catalogue entry, the URLs and the trace chain. Everything in memory (no disk).

use super::scan::{self, Found};
use super::*;
use crate::i18n::Lang;

const POST_RESOURCE: &str = include_str!(
    "../../../tests/fixtures/web/filament_app/app/Filament/Resources/PostResource.php"
);
const USER_RESOURCE: &str = include_str!(
    "../../../tests/fixtures/web/filament_app/app/Filament/Resources/UserResource.php"
);
const COMMENTS_MANAGER: &str = include_str!(
    "../../../tests/fixtures/web/filament_app/app/Filament/Resources/PostResource/RelationManagers/CommentsRelationManager.php"
);
const REVIEWS_MANAGER: &str = include_str!(
    "../../../tests/fixtures/web/filament_app/app/Filament/Resources/PostResource/RelationManagers/ReviewsRelationManager.php"
);
const PANEL: &str = include_str!(
    "../../../tests/fixtures/web/filament_app/app/Providers/Filament/AdminPanelProvider.php"
);
const SETTINGS: &str =
    include_str!("../../../tests/fixtures/web/filament_app/app/Filament/Pages/Settings.php");
const STATS: &str =
    include_str!("../../../tests/fixtures/web/filament_app/app/Filament/Widgets/StatsOverview.php");

const POST_PATH: &str = "app/Filament/Resources/PostResource.php";
const USER_PATH: &str = "app/Filament/Resources/UserResource.php";

fn post_info(columns: Option<&[&str]>) -> ModelInfo {
    ModelInfo {
        class: "Post".to_owned(),
        table: "posts".to_owned(),
        file: "app/Models/Post.php".to_owned(),
        columns: columns.map(|names| names.iter().map(|n| (*n).to_owned()).collect()),
        fillable: [
            "title",
            "slug",
            "body",
            "author_id",
            "status",
            "published_at",
            "meta",
        ]
        .map(str::to_owned)
        .to_vec(),
        casts: vec!["published_at".to_owned(), "meta".to_owned()],
        relations: vec![
            ("author".to_owned(), Some("User".to_owned())),
            ("comments".to_owned(), Some("Comment".to_owned())),
        ],
        members: Some(vec![
            "author".to_owned(),
            "comments".to_owned(),
            "getreadingtimeattribute".to_owned(),
            "publish".to_owned(),
        ]),
        opaque: false,
    }
}

fn user_info() -> ModelInfo {
    ModelInfo {
        class: "User".to_owned(),
        table: "users".to_owned(),
        file: "app/Models/User.php".to_owned(),
        columns: Some(vec!["id".to_owned(), "name".to_owned(), "email".to_owned()]),
        relations: vec![("posts".to_owned(), Some("Post".to_owned()))],
        members: Some(vec!["posts".to_owned()]),
        ..ModelInfo::default()
    }
}

const COLUMNS: [&str; 9] = [
    "id",
    "title",
    "slug",
    "body",
    "author_id",
    "status",
    "published_at",
    "meta",
    "created_at",
];

/// The fixture project in memory, every file in scope.
fn project() -> Filament {
    let files = [
        (POST_PATH, POST_RESOURCE),
        (USER_PATH, USER_RESOURCE),
        (
            "app/Filament/Resources/PostResource/RelationManagers/CommentsRelationManager.php",
            COMMENTS_MANAGER,
        ),
        (
            "app/Filament/Resources/PostResource/RelationManagers/ReviewsRelationManager.php",
            REVIEWS_MANAGER,
        ),
        ("app/Providers/Filament/AdminPanelProvider.php", PANEL),
        ("app/Filament/Pages/Settings.php", SETTINGS),
        ("app/Filament/Widgets/StatsOverview.php", STATS),
    ];
    let mut out = Filament::default();
    for (rel, text) in files {
        if let Some(found) = scan::scan_file(rel, text) {
            out.add(found);
        }
        out.scope.insert(rel.to_owned());
    }
    out
}

fn lookup(columns: Option<&'static [&'static str]>) -> impl Fn(&str) -> Option<ModelInfo> {
    move |short: &str| match short {
        "Post" => Some(post_info(columns)),
        "User" => Some(user_info()),
        "Comment" => Some(ModelInfo {
            class: "Comment".to_owned(),
            table: "comments".to_owned(),
            ..ModelInfo::default()
        }),
        _ => None,
    }
}

fn resource(text: &str, rel: &str) -> scan::Resource {
    match scan::scan_file(rel, text) {
        Some(Found::Resource(resource)) => *resource,
        other => panic!("not a resource: {other:?}"),
    }
}

fn names(fields: &[scan::Field]) -> Vec<String> {
    fields
        .iter()
        .map(|f| format!("{}{}", f.name, if f.required { "*" } else { "" }))
        .collect()
}

#[test]
fn resource_scan_reads_model_pages_and_relations() {
    let post = resource(POST_RESOURCE, POST_PATH);
    assert_eq!(post.name, "PostResource");
    assert_eq!(post.model.as_deref(), Some("Post"));
    let pages: Vec<(&str, &str, &str)> = post
        .pages
        .iter()
        .map(|p| (p.key.as_str(), p.class.as_str(), p.path.as_str()))
        .collect();
    assert_eq!(
        pages,
        [
            ("index", "ListPosts", "/"),
            ("create", "CreatePost", "/create"),
            ("edit", "EditPost", "/{record}/edit")
        ]
    );
    assert_eq!(
        post.relations,
        ["CommentsRelationManager", "ReviewsRelationManager"]
    );
}

#[test]
fn form_fields_come_from_nested_schemas_but_not_from_repeater_items() {
    let post = resource(POST_RESOURCE, POST_PATH);
    // Section -> Grid -> fields, a Fieldset, a relationship select; the placeholder and the repeater items are none
    assert_eq!(
        names(&post.form),
        [
            "title*",
            "slug*",
            "body",
            "tilte",
            "reading_time",
            "author_id*",
            "published_at",
            "meta"
        ]
    );
    let author = post.form.iter().find(|f| f.name == "author_id").unwrap();
    assert_eq!(author.relation.as_deref(), Some("author"));
    assert!(!author.check);
    let hidden = post.form.iter().find(|f| f.name == "reading_time").unwrap();
    assert!(!hidden.check, "dehydrated(false) is never checked");
}

#[test]
fn table_columns_filters_and_actions() {
    let post = resource(POST_RESOURCE, POST_PATH);
    let columns: Vec<&str> = post.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        columns,
        [
            "title",
            "author.name",
            "status",
            "reading_time",
            "comments_count",
            "published_at"
        ]
    );
    assert_eq!(post.filters, ["status"]);
    assert_eq!(post.actions, ["edit", "delete", "publish"]);
    assert_eq!(post.bulk, ["delete"]);
}

#[test]
fn a_name_built_at_run_time_is_skipped_and_counted() {
    let user = resource(USER_RESOURCE, USER_PATH);
    assert_eq!(names(&user.form), ["name*", "email*"]);
    assert_eq!(user.dynamic.len(), 1);
    assert_eq!(user.dynamic[0].0, "fi-field");
    assert_eq!(user.slug.as_deref(), Some("team"));
}

#[test]
fn panel_provider_gives_id_path_and_discovery() {
    let Some(Found::Panel(panel)) =
        scan::scan_file("app/Providers/Filament/AdminPanelProvider.php", PANEL)
    else {
        panic!("no panel");
    };
    assert_eq!(panel.id.as_deref(), Some("admin"));
    assert_eq!(panel.path.as_deref(), Some("admin"));
    assert_eq!(panel.discover, ["app/Filament/Resources"]);
    assert!(panel.dynamic.is_empty());
}

#[test]
fn a_widget_with_an_aliased_parent_is_found() {
    let Some(Found::Widget(widget)) =
        scan::scan_file("app/Filament/Widgets/StatsOverview.php", STATS)
    else {
        panic!("no widget");
    };
    assert_eq!(widget.kind, scan::WidgetKind::Stats);
    assert!(widget.idents.iter().any(|i| i == "Post"));
}

#[test]
fn contract_findings_are_the_wrong_field_and_the_wrong_relation_only() {
    let filament = project();
    let models = lookup(Some(&COLUMNS));
    let post = &filament.resources[0];
    assert_eq!(post.name, "PostResource");
    let warnings = filament.warnings(post, &models, Lang::En);
    let texts: Vec<&str> = warnings.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "form field 'tilte' is not a column of Post(posts)",
            "ReviewsRelationManager: Post(posts) has no relation 'reviews'"
        ]
    );
    assert_eq!(warnings[0].line, 41);
    assert_eq!(warnings[1].line, 7);
    // the dotted relation column, the accessor, the aggregate and `dehydrated(false)` are fine; the user resource too
    let user = &filament.resources[1];
    assert!(filament.warnings(user, &models, Lang::En).is_empty());
}

#[test]
fn no_column_findings_without_a_migration() {
    let filament = project();
    let models = lookup(None);
    let warnings = filament.warnings(&filament.resources[0], &models, Lang::En);
    let texts: Vec<&str> = warnings.iter().map(|w| w.text.as_str()).collect();
    // the relation findings need no migration, the column ones do
    assert_eq!(
        texts,
        ["ReviewsRelationManager: Post(posts) has no relation 'reviews'"]
    );
}

#[test]
fn an_opaque_model_gets_no_findings() {
    let filament = project();
    let models = |short: &str| {
        (short == "Post").then(|| ModelInfo {
            opaque: true,
            ..post_info(Some(&COLUMNS))
        })
    };
    assert!(
        filament
            .warnings(&filament.resources[0], &models, Lang::En)
            .is_empty()
    );
}

#[test]
fn a_wrong_dotted_column_is_reported_against_the_relation_and_its_target() {
    let text = POST_RESOURCE
        .replace("'author.name'", "'writer.name'")
        .replace("'status')->badge()", "'author.nick')->badge()");
    let mut filament = Filament::default();
    filament.add(scan::scan_file(POST_PATH, &text).unwrap());
    filament.scope.insert(POST_PATH.to_owned());
    let models = lookup(Some(&COLUMNS));
    let texts: Vec<String> = filament
        .warnings(&filament.resources[0], &models, Lang::En)
        .into_iter()
        .map(|w| w.text)
        .collect();
    assert!(
        texts.contains(
            &"table column 'writer.name': Post(posts) has no relation 'writer'".to_owned()
        ),
        "{texts:?}"
    );
    assert!(
        texts.contains(
            &"table column 'author.nick': 'nick' is not a column of User(users)".to_owned()
        ),
        "{texts:?}"
    );
}

#[test]
fn urls_follow_the_panel_path_the_slug_and_the_page_paths() {
    let filament = project();
    assert_eq!(filament.base_uri(&filament.resources[0]), "/admin/posts");
    assert_eq!(filament.base_uri(&filament.resources[1]), "/admin/team");
    assert_eq!(
        filament.pages_text(&filament.resources[0]),
        "index /admin/posts · create · edit /{record}/edit"
    );
}

#[test]
fn the_view_has_a_block_per_resource_and_the_standalone_items() {
    let filament = project();
    let models = lookup(Some(&COLUMNS));
    let lines = filament.lines(Lang::En, &models);
    assert_eq!(
        lines[0],
        "filament: resources 2, pages 1, widgets 1, panels 1, warnings 2"
    );
    let text = lines.join("\n");
    assert!(
        text.contains("PostResource  app/Filament/Resources/PostResource.php → Post(posts)\n"),
        "{text}"
    );
    assert!(
        text.contains("  form: title*, slug*, body, tilte, reading_time, author_id* (author), published_at, meta\n"),
        "{text}"
    );
    assert!(
        text.contains("  table: title, author.name, status, reading_time, comments_count, published_at · filters: status · actions: edit, delete, publish · bulk: delete\n"),
        "{text}"
    );
    assert!(
        text.contains("  relations: CommentsRelationManager, ReviewsRelationManager\n"),
        "{text}"
    );
    assert!(
        text.contains("page Settings  app/Filament/Pages/Settings.php · /admin/settings · view filament.pages.settings"),
        "{text}"
    );
    assert!(
        text.contains("widget StatsOverview (stats)  app/Filament/Widgets/StatsOverview.php · models: Post(posts), Comment(comments)"),
        "{text}"
    );
    assert!(
        text.contains("panel admin /admin  app/Providers/Filament/AdminPanelProvider.php · resources: PostResource, UserResource"),
        "{text}"
    );
    assert!(
        text.contains("? 1 skipped: form field name is a run-time value (app/Filament/Resources/UserResource.php L26)"),
        "{text}"
    );
}

#[test]
fn trace_names_model_pages_form_and_relation_managers() {
    let filament = project();
    let models = lookup(Some(&COLUMNS));
    for target in ["filament:posts", "PostResource", "filament:Post", POST_PATH] {
        let lines = filament
            .trace_lines(target, Lang::En, &models)
            .unwrap_or_else(|| panic!("{target}"));
        assert!(
            lines[0].starts_with("filament PostResource  app/Filament/Resources/PostResource.php:")
        );
        assert_eq!(lines[1], "model Post(posts)  app/Models/Post.php");
        assert!(lines[2].starts_with("panel admin /admin  "), "{lines:?}");
        assert!(
            lines[3].starts_with("pages index /admin/posts (ListPosts), create /admin/posts/create (CreatePost), edit /admin/posts/{record}/edit (EditPost)"),
            "{lines:?}"
        );
    }
    assert!(
        filament
            .trace_lines("filament:team", Lang::En, &models)
            .is_some()
    );
    assert!(
        filament
            .trace_lines("filament:nothing", Lang::En, &models)
            .is_none()
    );
    assert_eq!(
        filament.trace_candidates(),
        ["filament:posts", "filament:team"]
    );
}

#[test]
fn the_graph_key_names_the_model() {
    let calls = php_calls(POST_PATH, POST_RESOURCE);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].target, "filament::resource:Post");
    assert!(php_calls("app/Models/Post.php", "<?php class Post {}").is_empty());
}

#[test]
fn a_model_with_a_foreign_trait_or_call_is_opaque() {
    let (members, opaque) = scan::model_members(
        "<?php\nclass A extends Model {\n    use HasFactory, SoftDeletes;\n    public function getFullNameAttribute() {}\n}",
    );
    assert!(!opaque);
    assert!(members.contains(&"getfullnameattribute".to_owned()));
    let (_, opaque) = scan::model_members("<?php\nclass A extends Model {\n    use HasRoles;\n}");
    assert!(opaque);
    let (_, opaque) = scan::model_members(
        "<?php\nclass A extends Model {\n    public function __call($m, $a) {}\n}",
    );
    assert!(opaque);
}

#[test]
fn accessor_names_match_without_case_and_underscores() {
    let info = post_info(Some(&COLUMNS));
    assert!(info.has_attr("reading_time"));
    assert!(info.has_attr("author"));
    assert!(info.has_attr("published_at"));
    assert!(!info.has_attr("tilte"));
}
