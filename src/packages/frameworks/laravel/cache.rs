//! Lemezes cache a Laravel-kötések levezetett adataihoz (`.callgraph/laravel.bin`, MessagePack + LZ4, mint a
//! csomagtérkép cache-e): az egy-fájlos outline minden hívásnál újra elemezné a route-fájlokat, a `useForm`-os Vue
//! fájlokat, a controllert + FormRequest-et és az összes migrációt — ezek eredménye itt él, a bemeneti fájlok
//! bélyege (mtime + méret + ctime) szerint érvényesítve. Csak ha a `.callgraph/` már létező, közönséges mappa;
//! különben nincs cache, és a kimenet bájtra ugyanaz. Minden hiba csendes: sérült / régi cache = újraépítés.
//!
//! Tartalom:
//! - route-tábla: a `routes/**/*.php` + `bootstrap/app.php` + `app/Providers/**` ujjlenyomata (a fájllista is
//!   beleszámít, így hozzáadott / törölt fájl érvényteleníti);
//! - séma: a `database/migrations/*.php` ujjlenyomata;
//! - `useForm` küldések: JS / Vue fájlonként (út → bélyeg + küldések);
//! - validáció: PHP fájlonként (controller-metódus infó, FormRequest szabályai).
//!
//! Párhuzamos írók: ideiglenes fájl + átnevezés (`DirCache::save`); a legutóbbi író nyer, az érvényesítés
//! bélyeg-alapú, ezért az elavult tartalom sosem kerül felhasználásra.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::super::files::{ProjectFiles, Stamp};
use super::forms::Site;
use super::routes::Route;
use super::rules::Validation;
use super::schema::Schema;
use super::{Laravel, models::MIGRATIONS_DIR};
use crate::model::FileGraph;
use crate::packages::cache::{CacheStore, DirCache, decode, encode};

/// A cache fájlneve a `.callgraph/` mappában.
const FILE: &str = "laravel.bin";
/// A tárolt szerkezet verziója; ha változik (vagy új build jön), a régi cache csendben újraépül.
const FORMAT: u32 = 1;

/// Egy bemenet-halmaz ujjlenyomata: a gyökérhez relatív utak és bélyegeik, rendezve.
pub(crate) type Fingerprint = Vec<(String, Stamp)>;

/// A route-tábla levezetett adata (a `Laravel::scan` kimenete a `by_name` nélkül).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct RouteTable {
    pub routes: Vec<Route>,
    pub dynamic_names: Vec<(String, u32)>,
    pub unloaded: Vec<String>,
}

/// A migrációkból épített séma és a kétszeri létrehozások.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct SchemaTable {
    pub schema: Schema,
    pub duplicates: Vec<(String, String, String)>,
}

/// Egy controller-metódus validációs infója: az inline szabályok és a teljes nevű paraméter-típusok.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct MethodInfo {
    pub validation: Validation,
    pub types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SiteFile {
    stamp: Stamp,
    sites: Vec<SiteRepr>,
}

/// A `Site` tárolt alakja: az ige saját `String` (a `Site.verb` `&'static str`, az nem deszerializálható
/// közvetlenül); visszaolvasáskor csak a négy ismert ige fogadható el (más: sérült bejegyzés, kimarad).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SiteRepr {
    file: String,
    line: u32,
    target: String,
    caller: Option<String>,
    container: Option<String>,
    verb: String,
    fields: Vec<String>,
    arg: String,
}

impl SiteRepr {
    fn of(site: &Site) -> Self {
        Self {
            file: site.file.clone(),
            line: site.line,
            target: site.target.clone(),
            caller: site.caller.clone(),
            container: site.container.clone(),
            verb: site.verb.to_owned(),
            fields: site.fields.clone(),
            arg: site.arg.clone(),
        }
    }

    fn site(&self) -> Option<Site> {
        let verb = ["POST", "PUT", "PATCH", "DELETE"]
            .into_iter()
            .find(|verb| *verb == self.verb)?;
        Some(Site {
            file: self.file.clone(),
            line: self.line,
            target: self.target.clone(),
            caller: self.caller.clone(),
            container: self.container.clone(),
            verb,
            fields: self.fields.clone(),
            arg: self.arg.clone(),
        })
    }
}

/// Egy PHP fájl validációs adatai (lustán töltődnek: metódusonként, illetve FormRequest-ként).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RulesFile {
    stamp: Stamp,
    methods: BTreeMap<String, Option<MethodInfo>>,
    request: Option<Validation>,
}

impl RulesFile {
    fn new(stamp: Stamp) -> Self {
        Self {
            stamp,
            methods: BTreeMap::new(),
            request: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Stored {
    format: u32,
    version: String,
    routes: Option<(Fingerprint, RouteTable)>,
    schema: Option<(Fingerprint, SchemaTable)>,
    sites: BTreeMap<String, SiteFile>,
    rules: BTreeMap<String, RulesFile>,
}

impl Stored {
    fn empty(version: &str) -> Self {
        Self {
            format: FORMAT,
            version: version.to_owned(),
            routes: None,
            schema: None,
            sites: BTreeMap::new(),
            rules: BTreeMap::new(),
        }
    }
}

/// A betöltött cache; a módosítások a `flush`-ig memóriában gyűlnek.
pub(crate) struct LaravelCache {
    store: Box<dyn CacheStore + Send>,
    data: Stored,
    dirty: bool,
}

impl std::fmt::Debug for LaravelCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LaravelCache")
    }
}

/// A megosztott cache (a `Laravel` klónozható és szálak közt megosztható).
pub(crate) type Handle = Arc<Mutex<LaravelCache>>;

/// A jelenlegi build verziója: a formátum + a build ujjlenyomata.
fn current_version() -> String {
    format!("{FORMAT}+{}", crate::io::tool_version())
}

impl LaravelCache {
    /// Betöltés a tárolóból; hiányzó / sérült / más verziójú tartalom: üres cache.
    pub(super) fn open(store: Box<dyn CacheStore + Send>, version: &str) -> Self {
        let data = store
            .load()
            .and_then(|bytes| decode::<Stored>(&bytes))
            .filter(|stored| stored.format == FORMAT && stored.version == version)
            .unwrap_or_else(|| Stored::empty(version));
        Self {
            store,
            data,
            dirty: false,
        }
    }

    /// Kiírás, ha változott valami (a hiba nem számít: legközelebb újraépül).
    pub(super) fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if let Some(bytes) = encode(&self.data) {
            self.store.save(&bytes);
        }
    }

    fn routes(&self, inputs: &Fingerprint) -> Option<RouteTable> {
        let (stored, table) = self.data.routes.as_ref()?;
        (stored == inputs).then(|| table.clone())
    }

    fn put_routes(&mut self, inputs: Fingerprint, table: RouteTable) {
        self.data.routes = Some((inputs, table));
        self.dirty = true;
    }

    fn schema(&self, inputs: &Fingerprint) -> Option<SchemaTable> {
        let (stored, table) = self.data.schema.as_ref()?;
        (stored == inputs).then(|| table.clone())
    }

    fn put_schema(&mut self, inputs: Fingerprint, table: SchemaTable) {
        self.data.schema = Some((inputs, table));
        self.dirty = true;
    }

    fn sites(&self, rel: &str, stamp: Stamp) -> Option<Vec<Site>> {
        let entry = self.data.sites.get(rel)?;
        if entry.stamp != stamp {
            return None;
        }
        // egy hibás ige a teljes bejegyzést érvényteleníti (újraépül)
        entry.sites.iter().map(SiteRepr::site).collect()
    }

    fn put_sites(&mut self, rel: &str, stamp: Stamp, sites: Vec<Site>) {
        let sites = sites.iter().map(SiteRepr::of).collect();
        self.data
            .sites
            .insert(rel.to_owned(), SiteFile { stamp, sites });
        self.dirty = true;
    }

    /// A már nem létező JS / Vue fájlok bejegyzéseinek törlése.
    fn retain_sites(&mut self, present: &BTreeSet<String>) {
        let before = self.data.sites.len();
        self.data.sites.retain(|rel, _| present.contains(rel));
        self.dirty |= self.data.sites.len() != before;
    }

    fn method(&self, rel: &str, stamp: Stamp, method: &str) -> Option<Option<MethodInfo>> {
        let entry = self.data.rules.get(rel).filter(|e| e.stamp == stamp)?;
        entry.methods.get(method).cloned()
    }

    fn put_method(&mut self, rel: &str, stamp: Stamp, method: &str, info: Option<MethodInfo>) {
        self.rules_entry(rel, stamp)
            .methods
            .insert(method.to_owned(), info);
        self.dirty = true;
    }

    fn request(&self, rel: &str, stamp: Stamp) -> Option<Validation> {
        let entry = self.data.rules.get(rel).filter(|e| e.stamp == stamp)?;
        entry.request.clone()
    }

    fn put_request(&mut self, rel: &str, stamp: Stamp, validation: Validation) {
        self.rules_entry(rel, stamp).request = Some(validation);
        self.dirty = true;
    }

    /// A fájl bejegyzése; ha a bélyeg eltér, üresre cserélve.
    fn rules_entry(&mut self, rel: &str, stamp: Stamp) -> &mut RulesFile {
        let entry = self
            .data
            .rules
            .entry(rel.to_owned())
            .or_insert_with(|| RulesFile::new(stamp));
        if entry.stamp != stamp {
            *entry = RulesFile::new(stamp);
        }
        entry
    }
}

/// PHP-fájl-e az út.
fn is_php(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "php")
}

impl Laravel {
    /// A `.callgraph/laravel.bin` bekapcsolása (ha a `.callgraph/` létezik); különben marad a cache nélküli út.
    pub(crate) fn attach_disk_cache(&mut self) {
        let store = DirCache::named(&self.root, FILE);
        if store.usable() {
            self.attach_cache(Box::new(store), &current_version());
        }
    }

    /// Cache bekapcsolása tetszőleges tárolóval (DI: a tesztek memóriában tartják).
    pub(super) fn attach_cache(&mut self, store: Box<dyn CacheStore + Send>, version: &str) {
        self.cache = Some(Arc::new(Mutex::new(LaravelCache::open(store, version))));
    }

    /// A cache kiírása és lekapcsolása (a megosztott `Laravel` nem hordozza tovább).
    pub(crate) fn detach_cache(&mut self) {
        if let Some(handle) = self.cache.take()
            && let Ok(mut cache) = handle.lock()
        {
            cache.flush();
        }
    }

    /// Hozzáférés a cache-hez; `None`, ha nincs bekapcsolva (vagy a zár megmérgeződött).
    fn with_cache<T>(&self, run: impl FnOnce(&mut LaravelCache) -> T) -> Option<T> {
        let handle = self.cache.as_ref()?;
        let mut guard = handle.lock().ok()?;
        Some(run(&mut guard))
    }

    /// A fájlok ujjlenyomata: a `listed` minden fájljának bélyege kell (különben `None`: nem cache-elhető), az
    /// `optional` fájlok csak ha léteznek (a megjelenésük / eltűnésük így is változtat az ujjlenyomaton).
    fn fingerprint(
        &self,
        files: &dyn ProjectFiles,
        listed: &[PathBuf],
        optional: &[PathBuf],
    ) -> Option<Fingerprint> {
        let mut out: Fingerprint = Vec::with_capacity(listed.len() + optional.len());
        for path in listed {
            out.push((self.relative_path(path)?, files.stamp(path)?));
        }
        for path in optional {
            if let Some(stamp) = files.stamp(path) {
                out.push((self.relative_path(path)?, stamp));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Some(out)
    }

    /// A route-tábla bemeneteinek ujjlenyomata (a beolvasás előtt kell kérni: egy közben változó fájl így a
    /// következő hívásnál érvénytelenít). `None`: nincs cache, vagy a fájlrendszer nem ad bélyeget.
    pub(crate) fn route_fingerprint(&self, files: &dyn ProjectFiles) -> Option<Fingerprint> {
        self.cache.as_ref()?;
        let mut listed: Vec<PathBuf> = files
            .files_under(&self.root.join("routes"))
            .into_iter()
            .filter(|path| is_php(path))
            .collect();
        listed.extend(files.files_under(&self.root.join("app/Providers")));
        self.fingerprint(files, &listed, &[self.root.join("bootstrap/app.php")])
    }

    /// A cache-elt route-tábla alkalmazása; `true`, ha naprakész volt (a `scan` ilyenkor kihagyható).
    pub(crate) fn apply_cached_routes(&mut self, inputs: Option<&Fingerprint>) -> bool {
        let Some(table) = inputs.and_then(|inputs| self.with_cache(|c| c.routes(inputs)).flatten())
        else {
            return false;
        };
        self.routes = table.routes;
        self.dynamic_names = table.dynamic_names;
        self.unloaded = table.unloaded;
        self.rebuild_names();
        true
    }

    /// A frissen épített route-tábla mentése a (beolvasás előtt kért) ujjlenyomattal.
    pub(crate) fn store_routes(&self, inputs: Option<Fingerprint>) {
        let Some(inputs) = inputs else {
            return;
        };
        let table = RouteTable {
            routes: self.routes.clone(),
            dynamic_names: self.dynamic_names.clone(),
            unloaded: self.unloaded.clone(),
        };
        self.with_cache(|c| c.put_routes(inputs, table));
    }

    /// A migrációk (php) fájljai és ujjlenyomatuk (`None`: nincs cache / nincs bélyeg).
    pub(super) fn schema_fingerprint(
        &self,
        files: &dyn ProjectFiles,
    ) -> (Vec<PathBuf>, Option<Fingerprint>) {
        let paths: Vec<PathBuf> = files
            .files_under(&self.root.join(MIGRATIONS_DIR))
            .into_iter()
            .filter(|path| is_php(path))
            .collect();
        let inputs = self
            .cache
            .as_ref()
            .and_then(|_| self.fingerprint(files, &paths, &[]));
        (paths, inputs)
    }

    pub(super) fn cached_schema(&self, inputs: Option<&Fingerprint>) -> Option<SchemaTable> {
        let inputs = inputs?;
        self.with_cache(|c| c.schema(inputs))?
    }

    pub(super) fn store_schema(&self, inputs: Option<Fingerprint>, table: SchemaTable) {
        if let Some(inputs) = inputs {
            self.with_cache(|c| c.put_schema(inputs, table));
        }
    }

    /// A `useForm` küldések a JS / Vue fájlokból, fájlonként cache-elve (bélyeg-egyezéskor sem olvasás, sem
    /// elemzés). `analyze`: a fájl szűkített gráfja. A sorrend a `paths` sorrendje. Cache nélkül ugyanazt számolja.
    pub(crate) fn js_sites(
        &self,
        paths: &[PathBuf],
        files: &dyn ProjectFiles,
        analyze: &dyn Fn(&Path) -> Option<FileGraph>,
    ) -> Vec<Site> {
        let mut out = Vec::new();
        let mut present = BTreeSet::new();
        for path in paths {
            let Some(rel) = self.relative_path(path) else {
                continue;
            };
            let stamp = self.cache.as_ref().and_then(|_| files.stamp(path));
            if let Some(stamp) = stamp {
                present.insert(rel.clone());
                if let Some(hit) = self.with_cache(|c| c.sites(&rel, stamp)).flatten() {
                    out.extend(hit);
                    continue;
                }
            }
            let sites = Self::file_sites(&rel, path, files, analyze);
            if let Some(stamp) = stamp {
                self.with_cache(|c| c.put_sites(&rel, stamp, sites.clone()));
            }
            out.extend(sites);
        }
        // a törölt / átnevezett fájlok bejegyzései ne gyűljenek
        if !present.is_empty() {
            self.with_cache(|c| c.retain_sites(&present));
        }
        out
    }

    /// Egy JS / Vue fájl küldései: csak ha szerepel benne a `useForm`, és a gráfja modul `useForm` hívással.
    fn file_sites(
        rel: &str,
        path: &Path,
        files: &dyn ProjectFiles,
        analyze: &dyn Fn(&Path) -> Option<FileGraph>,
    ) -> Vec<Site> {
        let Some(text) = files.read(path).filter(|text| text.contains("useForm")) else {
            return Vec::new();
        };
        let Some(graph) = analyze(path) else {
            return Vec::new();
        };
        if graph.module.is_none()
            || !graph
                .calls
                .iter()
                .any(|call| call.resolved.as_deref() == Some(super::forms::USE_FORM))
        {
            return Vec::new();
        }
        super::forms::extract(rel, &text, &graph.calls)
    }

    /// Egy osztály fájlján számolt érték, a fájl bélyege szerint cache-elve. A `rels` az osztály lehetséges
    /// forrás-útjai (gyökérhez relatívan); az első létező számít. `None`: egyik sem olvasható.
    fn class_cached<T: Clone>(
        &self,
        rels: &[String],
        files: &dyn ProjectFiles,
        get: impl Fn(&LaravelCache, &str, Stamp) -> Option<T>,
        put: impl Fn(&mut LaravelCache, &str, Stamp, T),
        compute: impl Fn(&str) -> T,
    ) -> Option<T> {
        let stamped = self.cache.as_ref().and_then(|_| {
            rels.iter()
                .find_map(|rel| Some((rel, files.stamp(&self.root.join(rel))?)))
        });
        if let Some((rel, stamp)) = stamped {
            if let Some(hit) = self.with_cache(|c| get(c, rel, stamp)).flatten() {
                return Some(hit);
            }
            let text = files.read(&self.root.join(rel))?;
            let value = compute(&text);
            self.with_cache(|c| put(c, rel, stamp, value.clone()));
            return Some(value);
        }
        let text = rels
            .iter()
            .find_map(|rel| files.read(&self.root.join(rel)))?;
        Some(compute(&text))
    }

    /// A controller-metódus validációs infója (cache-elve); `None`: nincs olvasható osztály-fájl.
    pub(super) fn cached_method(
        &self,
        rels: &[String],
        files: &dyn ProjectFiles,
        method: &str,
        compute: impl Fn(&str) -> Option<MethodInfo>,
    ) -> Option<Option<MethodInfo>> {
        self.class_cached(
            rels,
            files,
            |c, rel, stamp| c.method(rel, stamp, method),
            |c, rel, stamp, info| c.put_method(rel, stamp, method, info),
            compute,
        )
    }

    /// A FormRequest szabályai (cache-elve); `None`: nincs olvasható osztály-fájl.
    pub(super) fn cached_request(
        &self,
        rels: &[String],
        files: &dyn ProjectFiles,
        compute: impl Fn(&str) -> Validation,
    ) -> Option<Validation> {
        self.class_cached(
            rels,
            files,
            LaravelCache::request,
            |c, rel, stamp, validation| c.put_request(rel, stamp, validation),
            compute,
        )
    }
}
