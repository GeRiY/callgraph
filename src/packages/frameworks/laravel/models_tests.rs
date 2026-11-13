//! A modell-összefoglaló tesztjei memóriabeli fájlrendszeren: a séma a migrációkból (create / table változások,
//! `down()` figyelmen kívül, morphs / timestamps / softDeletes / rememberToken kibontás, `foreignId()->constrained()`),
//! a modell ↔ tábla (explicit `$table`, konvenció, többes szám), a fillable / casts / relációk, a kimenetek szövege
//! (EN / HU), a json és a `models` nézet sorai.

use std::path::PathBuf;

use super::Laravel;
use super::schema::{plural, snake, table_of_class};
use super::tests::{MemFiles, graph, root};
use crate::i18n::Lang;
use crate::model::{Container, FileGraph};

const NS: Option<&str> = Some("App\\Models");

/// Egy migráció: az `up()` és a `down()` törzse.
fn migration(up: &str, down: &str) -> String {
    format!(
        "<?php\nuse Illuminate\\Database\\Migrations\\Migration;\n\nreturn new class extends Migration\n{{\n    public function up(): void\n    {{\n{up}\n    }}\n\n    public function down(): void\n    {{\n{down}\n    }}\n}};\n"
    )
}

/// `Schema::create('t', function (Blueprint $table) { .. });` a megadott utasításokkal.
fn create(table: &str, statements: &[&str]) -> String {
    block("create", table, statements)
}

fn alter(table: &str, statements: &[&str]) -> String {
    block("table", table, statements)
}

fn block(kind: &str, table: &str, statements: &[&str]) -> String {
    let body: String = statements
        .iter()
        .map(|statement| format!("            $table->{statement};\n"))
        .collect();
    format!(
        "        Schema::{kind}('{table}', function (Blueprint $table) {{\n{body}        }});\n"
    )
}

/// Egy modell-fájl: `class X extends <szülő> { <törzs> }` az `App\Models` névtérben.
fn model(class: &str, extends: &str, body: &str) -> (String, String, String) {
    let text = format!(
        "<?php\n\nnamespace App\\Models;\n\nclass {class} extends {extends}\n{{\n{body}\n}}\n"
    );
    (class.to_owned(), extends.to_owned(), text)
}

const MODEL: &str = "Illuminate\\Database\\Eloquent\\Model";
const AUTH: &str = "Illuminate\\Foundation\\Auth\\User";

/// A migrációk (fájlnév-sorrendben) és a modellek szerint feltöltött Laravel-kötés.
fn project(migrations: &[String], models: &[(String, String, String)]) -> Laravel {
    let mut files: Vec<(String, String)> = Vec::new();
    for (index, text) in migrations.iter().enumerate() {
        files.push((
            format!("database/migrations/2024_01_01_{index:06}_m.php"),
            text.clone(),
        ));
    }
    let mut graphs: Vec<(PathBuf, FileGraph)> = Vec::new();
    for (class, extends, text) in models {
        let rel = format!("app/Models/{class}.php");
        files.push((rel.clone(), text.clone()));
        let mut g = graph(&format!("/p/{rel}"), None, &[], Vec::new());
        g.namespace = Some("App\\Models".into());
        g.containers.push(Container {
            kind: "class".into(),
            name: class.clone(),
            line: 1,
            end_line: u32::try_from(text.lines().count()).unwrap(),
            is_abstract: None,
            is_final: None,
            extends: Some(vec![extends.clone()]),
            implements: None,
            traits: None,
            doc: None,
        });
        graphs.push((root().join(&rel), g));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(rel, text)| (rel.as_str(), text.as_str()))
        .collect();
    let mem = MemFiles::new(&refs);
    let mut laravel = Laravel::new(&root());
    laravel.scan_models(
        graphs.iter().map(|(path, graph)| (path.as_path(), graph)),
        &mem,
    );
    laravel
}

fn lines(laravel: &Laravel, class: &str, lang: Lang) -> Vec<String> {
    laravel.model_lines(NS, class, lang)
}

fn table_line(laravel: &Laravel, class: &str) -> String {
    lines(laravel, class, Lang::En).remove(0)
}

#[test]
fn a_create_migration_gives_the_compact_column_line() {
    let users = migration(
        &create(
            "users",
            &[
                "id()",
                "string('name')",
                "string('email')->unique()",
                "timestamp('email_verified_at')->nullable()",
                "string('password')",
                "rememberToken()",
                "timestamps()",
            ],
        ),
        "        Schema::dropIfExists('users');",
    );
    let laravel = project(&[users], &[model("User", AUTH, "")]);
    assert_eq!(
        table_line(&laravel, "User"),
        "table: users · id, name, email!, email_verified_at:ts?, password, remember_token?, timestamps"
    );
}

#[test]
fn the_down_method_is_ignored_and_drop_if_exists_in_up_removes_the_table() {
    let first = migration(
        &(create("kepts", &["id()", "string('x')"]) + &create("gone", &["id()"])),
        "        Schema::dropIfExists('kepts');\n        Schema::dropIfExists('gone');",
    );
    let second = migration("        Schema::dropIfExists('gone');", "");
    let laravel = project(
        &[first, second],
        &[model("Kept", MODEL, ""), model("Gone", MODEL, "")],
    );
    assert_eq!(table_line(&laravel, "Kept"), "table: kepts · id, x");
    assert_eq!(
        table_line(&laravel, "Gone"),
        "table: gones (no migration found)"
    );
    assert_eq!(
        lines(&laravel, "Gone", Lang::Hu)[0],
        "table: gones (nincs migráció)"
    );
}

#[test]
fn table_alterations_add_drop_rename_and_change_columns() {
    let first = migration(
        &create(
            "posts",
            &[
                "id()",
                "string('title')",
                "text('body')",
                "integer('views')",
                "string('slug')->unique()",
                "string('legacy')",
            ],
        ),
        "",
    );
    let second = migration(
        &alter(
            "posts",
            &[
                "boolean('published')->default(false)",
                "dropColumn('legacy')",
                "dropColumn(['views', 'nothing'])",
                "renameColumn('slug', 'permalink')",
                "text('title')->nullable()->change()",
            ],
        ),
        &alter("posts", &["dropColumn('published')"]),
    );
    let laravel = project(&[first, second], &[model("Post", MODEL, "")]);
    assert_eq!(
        table_line(&laravel, "Post"),
        "table: posts · id, title:text?, body:text, permalink!, published:bool"
    );
}

#[test]
fn rename_table_and_alter_of_an_unknown_table_are_handled() {
    let first = migration(&create("old_names", &["id()"]), "");
    let second = migration("        Schema::rename('old_names', 'new_names');", "");
    let third = migration(&alter("ghost", &["string('x')"]), "");
    let laravel = project(
        &[first, second, third],
        &[model("NewName", MODEL, ""), model("Ghost", MODEL, "")],
    );
    assert_eq!(table_line(&laravel, "NewName"), "table: new_names · id");
    assert_eq!(
        table_line(&laravel, "Ghost"),
        "table: ghosts (no migration found)"
    );
}

#[test]
fn morphs_foreign_ids_and_modifiers_expand() {
    let things = migration(
        &create(
            "things",
            &[
                "id()",
                "morphs('taggable')",
                "nullableMorphs('owner')",
                "foreignId('user_id')->constrained()",
                "foreignId('cat')->nullable()->constrained('categories')",
                "foreignIdFor(Company::class)",
                "uuid('uid')->unique()",
                "enum('state', ['a', 'b'])",
                "decimal('price', 8, 2)",
                "string('code', 100)->nullable()->unique()->default('x')",
                "softDeletes()",
            ],
        ),
        "",
    );
    let laravel = project(&[things], &[model("Thing", MODEL, "")]);
    assert_eq!(
        table_line(&laravel, "Thing"),
        "table: things · id, taggable_type, taggable_id:bigint, owner_type?, owner_id:bigint?, user_id:fk→users, cat:fk→categories?, company_id:fk→companies, uid:uuid!, state:enum, price:dec, code?!, softDeletes"
    );
}

#[test]
fn foreign_calls_and_unparseable_statements_are_handled() {
    let notes = migration(
        &(create(
            "notes",
            &[
                "id()",
                "unsignedBigInteger('author')",
                "foreign('author')->references('id')->on('people')",
                "string('a')",
                "unique('a')",
                "index(['a', 'b'])",
                "whatever($x)->weird()",
            ],
        ) + "        if (Schema::hasColumn('notes', 'z')) { $z = 1; }\n"),
        "",
    );
    let laravel = project(&[notes], &[model("Note", MODEL, "")]);
    assert_eq!(
        table_line(&laravel, "Note"),
        "table: notes · id, author:fk→people, a!"
    );
}

#[test]
fn explicit_table_beats_the_convention() {
    let laravel = project(
        &[
            migration(&create("legacy_members", &["id()"]), ""),
            migration(&create("members", &["id()", "string('x')"]), ""),
        ],
        &[model(
            "Member",
            MODEL,
            "    protected $table = 'legacy_members';",
        )],
    );
    assert_eq!(table_line(&laravel, "Member"), "table: legacy_members · id");
}

#[test]
fn the_table_convention_pluralises_the_snake_case_class_name() {
    for (class, table) in [
        ("User", "users"),
        ("ServiceJob", "service_jobs"),
        ("Company", "companies"),
        ("Fee", "fees"),
        ("Status", "statuses"),
        ("ServiceJobsWorkflow", "service_jobs_workflows"),
        ("Person", "people"),
        ("Child", "children"),
        ("Box", "boxes"),
        ("Batch", "batches"),
        ("Dish", "dishes"),
        ("Day", "days"),
    ] {
        assert_eq!(table_of_class(class), table, "{class}");
    }
    assert_eq!(snake("ServiceJob"), "service_job");
    assert_eq!(plural("data"), "data");
}

#[test]
fn fillable_and_casts_are_listed_and_checked_against_the_columns() {
    let users = migration(
        &create("users", &["id()", "string('name')", "string('email')"]),
        "",
    );
    let body = "    protected $fillable = ['name', 'email', 'nick', 'foo'];\n    protected $casts = ['email' => 'string', 'old' => 'int'];\n\n    protected function casts(): array\n    {\n        return [\n            'name' => 'encrypted',\n            'extra' => AsArrayObject::class,\n        ];\n    }";
    let laravel = project(&[users], &[model("User", MODEL, body)]);
    assert_eq!(
        lines(&laravel, "User", Lang::En),
        vec![
            "table: users · id, name, email".to_owned(),
            "fillable: name, email, nick, foo · casts: email:string, old:int, name:encrypted, extra:AsArrayObject"
                .to_owned(),
            "⚠ fillable not a column: nick, foo".to_owned(),
            "⚠ cast not a column: old, extra".to_owned(),
        ]
    );
    let hu = lines(&laravel, "User", Lang::Hu);
    assert_eq!(hu[2], "⚠ fillable, de nincs ilyen oszlop: nick, foo");
    assert_eq!(hu[3], "⚠ cast, de nincs ilyen oszlop: old, extra");
}

#[test]
fn long_fillable_lists_are_capped_at_eight_names() {
    let names: Vec<String> = (1..=10).map(|n| format!("'c{n}'")).collect();
    let body = format!("    protected $fillable = [{}];", names.join(", "));
    let laravel = project(
        &[migration(&create("items", &["id()"]), "")],
        &[model("Item", MODEL, &body)],
    );
    let out = lines(&laravel, "Item", Lang::En);
    assert_eq!(out[1], "fillable: c1, c2, c3, c4, c5, c6, c7, c8, +2");
    assert_eq!(
        out[2],
        "⚠ fillable not a column: c1, c2, c3, c4, c5, c6, c7, c8, +2"
    );
}

const USER_BODY: &str = "    protected $fillable = ['name', 'nick'];

    public function roles(): BelongsToMany
    {
        return $this->belongsToMany(Role::class);
    }

    public function posts()
    {
        return $this->hasMany(\\App\\Models\\Post::class, 'author_id');
    }

    public function profile()
    {
        return $this->hasOne(Profile::class);
    }

    public function owner()
    {
        return $this->belongsTo(Company::class);
    }

    public function commentable()
    {
        return $this->morphTo();
    }

    public function countries()
    {
        return $this->hasManyThrough(Country::class, Company::class);
    }

    public function helper()
    {
        return 1;
    }";

fn users_project() -> Laravel {
    project(
        &[
            migration(
                &create("users", &["id()", "string('name')", "timestamps()"]),
                "",
            ),
            migration(&create("roles", &["id()", "string('name')"]), ""),
            migration(
                &create(
                    "role_user",
                    &["foreignId('user_id')", "foreignId('role_id')"],
                ),
                "",
            ),
            migration(&create("audits", &["id()"]), ""),
            migration(&create("sessions", &["string('id')"]), ""),
        ],
        &[model("User", AUTH, USER_BODY), model("Role", MODEL, "")],
    )
}

#[test]
fn relations_have_kind_and_target() {
    let laravel = users_project();
    let user = laravel
        .models()
        .iter()
        .find(|m| m.class == "App\\Models\\User")
        .unwrap();
    let got: Vec<(&str, &str, Option<&str>)> = user
        .relations
        .iter()
        .map(|r| (r.method.as_str(), r.kind.as_str(), r.target.as_deref()))
        .collect();
    assert_eq!(
        got,
        vec![
            ("roles", "belongsToMany", Some("Role")),
            ("posts", "hasMany", Some("Post")),
            ("profile", "hasOne", Some("Profile")),
            ("owner", "belongsTo", Some("Company")),
            ("commentable", "morphTo", None),
            ("countries", "hasManyThrough", Some("Country")),
        ]
    );
    assert_eq!(
        laravel.relation_target(NS, "User", "roles").as_deref(),
        Some("Role")
    );
    assert_eq!(laravel.relation_target(NS, "User", "commentable"), None);
    assert_eq!(laravel.relation_target(NS, "User", "helper"), None);
    assert_eq!(laravel.relation_target(NS, "Other", "roles"), None);
}

#[test]
fn the_models_view_lists_models_issues_and_orphan_tables() {
    let laravel = users_project();
    let scope = vec![
        "app/Models/User.php".to_owned(),
        "app/Models/Role.php".to_owned(),
    ];
    assert_eq!(
        laravel.model_view_lines(&scope, Lang::En),
        vec![
            "Role roles(2)".to_owned(),
            "User users(4) → roles:Role*, posts:Post*, profile:Profile, owner:Company, commentable:?, countries:Country*"
                .to_owned(),
            "⚠ User: fillable not a column: nick".to_owned(),
            "models: 2, tables without model: audits".to_owned(),
        ]
    );
    let hu = laravel.model_view_lines(&scope, Lang::Hu);
    assert_eq!(hu[2], "⚠ User: fillable, de nincs ilyen oszlop: nick");
    assert_eq!(hu[3], "modellek: 2, modell nélküli táblák: audits");
    // a hatókör szűr; modell nélküli hatókör: üres
    assert_eq!(
        laravel.model_view_lines(&["app/Models/Role.php".to_owned()], Lang::En)[0],
        "Role roles(2)"
    );
    assert!(
        laravel
            .model_view_lines(&["app/Http/X.php".to_owned()], Lang::En)
            .is_empty()
    );
}

#[test]
fn a_model_without_a_migration_shows_a_question_mark_count() {
    let laravel = project(&[], &[model("Lonely", MODEL, "")]);
    assert_eq!(
        laravel.model_view_lines(&["app/Models/Lonely.php".to_owned()], Lang::En),
        vec!["Lonely lonelies(?)".to_owned(), "models: 1".to_owned()]
    );
}

#[test]
fn indirect_subclasses_are_models_and_plain_classes_are_not() {
    let laravel = project(
        &[migration(&create("admins", &["id()"]), "")],
        &[
            model("BaseModel", MODEL, ""),
            model("Admin", "App\\Models\\BaseModel", ""),
            model("Helper", "App\\Support\\Thing", ""),
        ],
    );
    let classes: Vec<&str> = laravel.models().iter().map(|m| m.class.as_str()).collect();
    assert_eq!(
        classes,
        vec!["App\\Models\\Admin", "App\\Models\\BaseModel"]
    );
    assert_eq!(table_line(&laravel, "Admin"), "table: admins · id");
    assert!(laravel.model_lines(NS, "Helper", Lang::En).is_empty());
}

#[test]
fn the_json_value_carries_table_columns_casts_relations_and_issues() {
    let laravel = users_project();
    let json = laravel.model_json(NS, "User").unwrap();
    assert_eq!(json["table"], "users");
    assert_eq!(json["columns"][0]["name"], "id");
    assert_eq!(json["columns"][0]["type"], "id");
    assert_eq!(json["columns"][0]["nullable"], false);
    assert_eq!(json["columns"][2]["name"], "created_at");
    assert_eq!(json["columns"][2]["nullable"], true);
    assert_eq!(json["fillable"], serde_json::json!(["name", "nick"]));
    assert_eq!(
        json["relations"][0],
        serde_json::json!({"method": "roles", "kind": "belongsToMany", "target": "Role"})
    );
    assert_eq!(
        json["issues"],
        serde_json::json!(["fillable not a column: nick"])
    );
    assert!(json.get("casts").is_none());
    let pivot = laravel.model_json(NS, "Role").unwrap();
    assert_eq!(pivot["table"], "roles");
    assert!(laravel.model_json(NS, "Nope").is_none());
    let fk = project(
        &[migration(
            &create("pets", &["foreignId('owner_id')->constrained('people')"]),
            "",
        )],
        &[model("Pet", MODEL, "")],
    )
    .model_json(NS, "Pet")
    .unwrap();
    assert_eq!(fk["columns"][0]["fk"], "people");
}

#[test]
fn nothing_is_known_before_the_model_scan() {
    let laravel = Laravel::new(&root());
    assert!(laravel.model_lines(NS, "User", Lang::En).is_empty());
    assert!(laravel.model_view_lines(&[], Lang::En).is_empty());
    assert!(laravel.model_json(NS, "User").is_none());
}
