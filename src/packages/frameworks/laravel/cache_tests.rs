//! A lemezes cache (`laravel/cache.rs`) tesztjei memóriabeli fájlrendszeren és tárolón: az első hívás kiírja, a
//! második újrahasználja (a drága lépések — a route-fájl, a `useForm`-os Vue fájl, a controller, a migráció
//! olvasása / elemzése — nem futnak újra); a bemenet bélyegének (mtime / méret) változása, hozzáadott / törölt fájl
//! és más verzió érvényteleníti; a kimenet cache-sel és anélkül azonos; a sérült cache újraépül.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::super::files::{ProjectFiles, Stamp};
use super::super::targeted::{self, Need};
use super::Laravel;
use super::targeted_tests::{
    COMPOSER, CONTROLLER, CONTROLLER_TEXT, CONTROLLERS, MIGRATION, MODEL_TEXT, OTHER_VUE,
    REQUEST_TEXT, ROUTES, VUE, VUE_TEXT, analyzed, controller_graph, model_graph, vue_graph,
};
use super::tests::root;
use crate::i18n::Lang;
use crate::packages::cache::CacheStore;
use crate::query::QueryResult;

const REQUEST: &str = "app/Http/Requests/RegisterRequest.php";
const MIGRATION_FILE: &str = "database/migrations/2024_01_01_000000_posts.php";
const MODEL: &str = "app/Models/Post.php";

/// Memóriabeli fájlrendszer bélyegekkel és olvasás-számlálóval.
struct Disk {
    files: RefCell<BTreeMap<PathBuf, (String, Stamp)>>,
    reads: RefCell<Vec<PathBuf>>,
}

impl Disk {
    fn new(files: &[(&str, &str)]) -> Self {
        let disk = Self {
            files: RefCell::new(BTreeMap::new()),
            reads: RefCell::new(Vec::new()),
        };
        for (rel, text) in files {
            disk.put(rel, text);
        }
        disk
    }

    fn stamp_of(text: &str, mtime: u64) -> Stamp {
        Stamp {
            mtime,
            size: u64::try_from(text.len()).unwrap_or(u64::MAX),
            ctime: 0,
        }
    }

    /// Új tartalom új mtime-mal.
    fn put(&self, rel: &str, text: &str) {
        let path = root().join(rel);
        let mtime = self
            .files
            .borrow()
            .get(&path)
            .map_or(1, |(_, stamp)| stamp.mtime + 1);
        self.files
            .borrow_mut()
            .insert(path, (text.to_owned(), Self::stamp_of(text, mtime)));
    }

    /// Új tartalom a régi mtime-mal (csak a méret változik).
    fn put_same_mtime(&self, rel: &str, text: &str) {
        let path = root().join(rel);
        let mtime = self.files.borrow()[&path].1.mtime;
        self.files
            .borrow_mut()
            .insert(path, (text.to_owned(), Self::stamp_of(text, mtime)));
    }

    fn remove(&self, rel: &str) {
        self.files.borrow_mut().remove(&root().join(rel));
    }

    fn reads_of(&self, rel: &str) -> usize {
        let path = root().join(rel);
        self.reads.borrow().iter().filter(|p| **p == path).count()
    }

    fn clear_reads(&self) {
        self.reads.borrow_mut().clear();
    }
}

impl ProjectFiles for Disk {
    fn read(&self, path: &Path) -> Option<String> {
        self.reads.borrow_mut().push(path.to_path_buf());
        self.files.borrow().get(path).map(|(text, _)| text.clone())
    }

    fn files_under(&self, dir: &Path) -> Vec<PathBuf> {
        self.files
            .borrow()
            .keys()
            .filter(|path| path.starts_with(dir))
            .cloned()
            .collect()
    }

    fn stamp(&self, path: &Path) -> Option<Stamp> {
        self.files.borrow().get(path).map(|(_, stamp)| *stamp)
    }
}

/// Memóriabeli cache-tároló (a tesztek között megosztható klónokkal), mentés-számlálóval.
#[derive(Clone, Default)]
struct MemStore {
    bytes: Arc<Mutex<Option<Vec<u8>>>>,
    saves: Arc<AtomicUsize>,
}

impl MemStore {
    fn saves(&self) -> usize {
        self.saves.load(Ordering::SeqCst)
    }
}

impl CacheStore for MemStore {
    fn load(&self) -> Option<Vec<u8>> {
        self.bytes.lock().unwrap().clone()
    }

    fn save(&self, bytes: &[u8]) {
        *self.bytes.lock().unwrap() = Some(bytes.to_vec());
        self.saves.fetch_add(1, Ordering::SeqCst);
    }
}

fn project() -> Disk {
    Disk::new(&[
        ("composer.json", COMPOSER),
        ("routes/web.php", ROUTES),
        (CONTROLLER, CONTROLLER_TEXT),
        (REQUEST, REQUEST_TEXT),
        (VUE, VUE_TEXT),
        (OTHER_VUE, "<script setup>\nconst a = 1;\n</script>\n"),
        (MODEL, MODEL_TEXT),
        (MIGRATION_FILE, MIGRATION),
    ])
}

const ALL: Need = Need {
    inertia: false,
    routes: true,
    forms: true,
    models: true,
};

/// Egy célzott keresés (cache-sel, ha `store` van); a második: az elemzett fájlok száma.
fn scan(disk: &Disk, store: Option<(&MemStore, &str)>, rows: &[QueryResult]) -> (Laravel, usize) {
    let map = analyzed();
    let seen = RefCell::new(0_usize);
    let analyze = |path: &Path| {
        *seen.borrow_mut() += 1;
        map.get(path).cloned()
    };
    let mut laravel = Laravel::new(&root());
    if let Some((store, version)) = store {
        laravel.attach_cache(Box::new(store.clone()), version);
    }
    targeted::scan(rows, None, Some(&mut laravel), ALL, &root(), disk, &analyze);
    laravel.detach_cache();
    (laravel, seen.into_inner())
}

fn controller_rows() -> Vec<QueryResult> {
    vec![QueryResult::new(controller_graph(), None)]
}

fn vue_rows() -> Vec<QueryResult> {
    vec![QueryResult::new(vue_graph(), None)]
}

fn model_rows() -> Vec<QueryResult> {
    vec![QueryResult::new(model_graph("Model"), None)]
}

/// A kimenetek, amelyeket a cache nem változtathat meg.
fn snapshot(laravel: &Laravel) -> Vec<String> {
    let route = |method: &str| laravel.route_line(Some(CONTROLLERS), "RegisterController", method);
    let mut out = vec![
        format!("{:?}", route("store")),
        format!("{:?}", route("create")),
        format!(
            "{:?}",
            laravel.form_line(Some(CONTROLLERS), "RegisterController", "store")
        ),
        format!(
            "{:?}",
            laravel.form_lines(&format!("/p/{VUE}"), Some("Page"), "submit", Lang::En)
        ),
    ];
    out.extend(laravel.model_lines(Some("App\\Models"), "Post", Lang::En));
    out.push(format!(
        "{:?}",
        laravel.model_json(Some("App\\Models"), "Post")
    ));
    out
}

/// Minden sor egyben (controller + Vue + modell).
fn all_rows() -> Vec<QueryResult> {
    let mut rows = controller_rows();
    rows.extend(vue_rows());
    rows.extend(model_rows());
    rows
}

#[test]
fn the_first_call_writes_the_cache_and_the_second_reuses_it() {
    let disk = project();
    let store = MemStore::default();
    let rows = all_rows();
    let (first, first_analyzed) = scan(&disk, Some((&store, "v1")), &rows);
    assert_eq!(store.saves(), 1);
    assert!(first_analyzed > 0);
    disk.clear_reads();
    let (second, second_analyzed) = scan(&disk, Some((&store, "v1")), &rows);
    // a drága lépések nem futnak újra: se route-fájl, se Vue, se controller / FormRequest, se migráció olvasás
    for rel in ["routes/web.php", VUE, CONTROLLER, REQUEST, MIGRATION_FILE] {
        assert_eq!(disk.reads_of(rel), 0, "{rel}");
    }
    assert_eq!(second_analyzed, 0);
    assert_eq!(store.saves(), 1, "változatlan cache nem íródik újra");
    assert_eq!(snapshot(&second), snapshot(&first));
}

#[test]
fn the_output_is_identical_with_and_without_the_cache() {
    let rows = all_rows();
    let plain = scan(&project(), None, &rows).0;
    let store = MemStore::default();
    let disk = project();
    let cold = scan(&disk, Some((&store, "v1")), &rows).0;
    let warm = scan(&disk, Some((&store, "v1")), &rows).0;
    let expected = snapshot(&plain);
    assert!(expected[0].contains("POST /register"));
    assert_eq!(snapshot(&cold), expected);
    assert_eq!(snapshot(&warm), expected);
}

#[test]
fn no_cache_attached_leaves_the_store_untouched() {
    let store = MemStore::default();
    let _ = scan(&project(), None, &all_rows());
    assert_eq!(store.saves(), 0);
}

#[test]
fn a_changed_route_file_rebuilds_the_route_table() {
    let disk = project();
    let store = MemStore::default();
    let rows = controller_rows();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    let more = format!("{ROUTES}Route::put('register', [RegisterController::class, 'update']);\n");
    disk.put("routes/web.php", &more);
    disk.clear_reads();
    let (laravel, _) = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of("routes/web.php") > 0);
    assert!(
        laravel
            .route_line(Some(CONTROLLERS), "RegisterController", "update")
            .is_some_and(|line| line.contains("PUT /register"))
    );
    assert_eq!(store.saves(), 2);
    // és a következő hívás megint a cache-ből jön
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert_eq!(disk.reads_of("routes/web.php"), 0);
}

#[test]
fn a_size_only_change_invalidates_too() {
    let disk = project();
    let store = MemStore::default();
    let rows = controller_rows();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    disk.put_same_mtime("routes/web.php", &format!("{ROUTES}// x\n"));
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of("routes/web.php") > 0);
}

#[test]
fn an_added_or_removed_route_file_invalidates_the_route_table() {
    let disk = project();
    let store = MemStore::default();
    let rows = controller_rows();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    disk.put("routes/api.php", "<?php\n");
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of("routes/web.php") > 0, "hozzáadás");
    disk.remove("routes/api.php");
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of("routes/web.php") > 0, "törlés");
    // a belépési pont (bootstrap/app.php) megjelenése is számít: az dönti el, mely route-fájl töltődik be
    disk.put(
        "bootstrap/app.php",
        "<?php\n// withRouting(web: __DIR__.'/../routes/web.php')\n",
    );
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of("routes/web.php") > 0, "bootstrap");
}

#[test]
fn a_changed_migration_rebuilds_the_schema_and_an_unchanged_one_is_not_read() {
    let disk = project();
    let store = MemStore::default();
    let rows = model_rows();
    let (first, _) = scan(&disk, Some((&store, "v1")), &rows);
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert_eq!(disk.reads_of(MIGRATION_FILE), 0);
    let changed = MIGRATION.replace(
        "$table->timestamps();",
        "$table->string('slug');\n            $table->timestamps();",
    );
    disk.put(MIGRATION_FILE, &changed);
    disk.clear_reads();
    let (second, _) = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of(MIGRATION_FILE) > 0);
    let columns =
        |laravel: &Laravel| laravel.model_lines(Some("App\\Models"), "Post", Lang::En)[0].clone();
    assert!(!columns(&first).contains("slug"));
    assert!(columns(&second).contains("slug"));
    // új migráció is érvényteleníti
    disk.put(
        "database/migrations/2024_02_01_000000_more.php",
        "<?php
return new class extends Migration
{
    public function up(): void
    {
        Schema::table('posts', function (Blueprint $table) {
            $table->string('extra');
        });
    }

    public function down(): void
    {
    }
};
",
    );
    let (third, _) = scan(&disk, Some((&store, "v1")), &rows);
    assert!(columns(&third).contains("extra"));
}

#[test]
fn a_changed_vue_file_is_reanalyzed_and_the_others_are_not() {
    let disk = project();
    let store = MemStore::default();
    let rows = controller_rows();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    disk.put(VUE, &format!("{VUE_TEXT}\n"));
    disk.clear_reads();
    let (_, analyzed_count) = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of(VUE) > 0);
    assert_eq!(analyzed_count, 1);
    assert_eq!(disk.reads_of(OTHER_VUE), 0);
    // egy új, `useForm`-ot nem említő fájl is csak egyszer van olvasva
    disk.put("resources/js/Pages/New.vue", "<script setup>\n</script>\n");
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert_eq!(disk.reads_of("resources/js/Pages/New.vue"), 0);
}

#[test]
fn a_changed_form_request_changes_the_form_check() {
    let disk = project();
    let store = MemStore::default();
    let rows = vue_rows();
    let (first, _) = scan(&disk, Some((&store, "v1")), &rows);
    let form = |laravel: &Laravel| {
        laravel.form_lines(&format!("/p/{VUE}"), Some("Page"), "submit", Lang::En)
    };
    assert!(form(&first)[0].contains("unvalidated: legacy"));
    disk.put(
        REQUEST,
        &REQUEST_TEXT.replace(
            "'terms' => 'required',",
            "'terms' => 'required', 'legacy' => 'string',",
        ),
    );
    disk.clear_reads();
    let (second, _) = scan(&disk, Some((&store, "v1")), &rows);
    assert!(disk.reads_of(REQUEST) > 0);
    assert!(!form(&second)[0].contains("unvalidated: legacy"));
    // a controller-fájl változatlan: nem olvasta újra
    assert_eq!(disk.reads_of(CONTROLLER), 0);
}

#[test]
fn a_different_version_ignores_the_old_cache() {
    let disk = project();
    let store = MemStore::default();
    let rows = all_rows();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    disk.clear_reads();
    let (laravel, _) = scan(&disk, Some((&store, "v2")), &rows);
    assert!(disk.reads_of("routes/web.php") > 0);
    assert_eq!(store.saves(), 2);
    assert_eq!(
        snapshot(&laravel),
        snapshot(&scan(&project(), None, &rows).0)
    );
}

#[test]
fn a_corrupt_cache_is_rebuilt_without_error() {
    let disk = project();
    let store = MemStore::default();
    *store.bytes.lock().unwrap() = Some(b"definitely not a cache".to_vec());
    let rows = all_rows();
    let (laravel, _) = scan(&disk, Some((&store, "v1")), &rows);
    assert_eq!(
        snapshot(&laravel),
        snapshot(&scan(&project(), None, &rows).0)
    );
    assert_eq!(store.saves(), 1);
    disk.clear_reads();
    let _ = scan(&disk, Some((&store, "v1")), &rows);
    assert_eq!(disk.reads_of("routes/web.php"), 0);
}

#[test]
fn a_filesystem_without_stamps_never_caches() {
    // a `MemFiles`-szerű fájlrendszer nem ad bélyeget: nincs mit érvényesíteni, ezért nem is ír cache-t
    struct NoStamps(Disk);
    impl ProjectFiles for NoStamps {
        fn read(&self, path: &Path) -> Option<String> {
            self.0.read(path)
        }
        fn files_under(&self, dir: &Path) -> Vec<PathBuf> {
            self.0.files_under(dir)
        }
    }
    let disk = NoStamps(project());
    let store = MemStore::default();
    let map = analyzed();
    let analyze = |path: &Path| map.get(path).cloned();
    let mut laravel = Laravel::new(&root());
    laravel.attach_cache(Box::new(store.clone()), "v1");
    targeted::scan(
        &controller_rows(),
        None,
        Some(&mut laravel),
        ALL,
        &root(),
        &disk,
        &analyze,
    );
    laravel.detach_cache();
    assert_eq!(store.saves(), 0);
    assert!(
        laravel
            .route_line(Some(CONTROLLERS), "RegisterController", "store")
            .is_some()
    );
}
