//! Chain of Responsibility: a rövid osztálynév teljes névvé (FQCN) oldása láncolt feloldókkal, az első találatig:
//! 1. explicit `use:` (és a célfájl meglévő importjai) → 2. a projekt osztály-indexe (PSR-4 + fájlnév) →
//! 3. a projektben már használt importok (`use X\Y;` sorok) → 4. a PHP globális osztályai → 5. Laravel-projektben a
//!    keretrendszer gyakori osztályai → feloldatlan.
//!
//! A 2. és 3. lépés lustán halad: a jelölteket (fájlokat) folyamatosan adja, és az első találatnál megáll.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::fs::{FileSystem, MemoWalk, walk_php};
use super::project::{Psr4, relative};

/// A lánc egy tagja.
pub trait NameResolver {
    /// A rövid névhez tartozó összes (különböző) teljes név, amit ez a tag ismer (általában 0 vagy 1).
    fn candidates(&self, short: &str) -> Vec<String>;

    /// A rövid névhez a teljes név, ha ez a tag tudja (több jelölt esetén az első).
    fn resolve(&self, short: &str) -> Option<String> {
        self.candidates(short).into_iter().next()
    }
}

/// A rövid név feloldásának eredménye: egyértelmű, nem egyértelmű (több azonos nevű osztály) vagy ismeretlen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    One(String),
    /// Legalább két különböző teljes név (rendezve).
    Ambiguous(Vec<String>),
    Unknown,
}

fn push_unique(out: &mut Vec<String>, fqcn: String) {
    if !out.contains(&fqcn) {
        out.push(fqcn);
    }
}

/// 1. Explicit importok: `(alias, FQCN)` párok (a kifejezés `use:` sorai és a fájl meglévő importjai).
pub struct ExplicitUse {
    entries: Vec<(String, String)>,
}

impl ExplicitUse {
    pub fn new(entries: Vec<(String, String)>) -> Self {
        Self { entries }
    }
}

impl NameResolver for ExplicitUse {
    fn candidates(&self, short: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (_, fqcn) in self.entries.iter().filter(|(alias, _)| alias == short) {
            push_unique(&mut out, fqcn.clone());
        }
        out
    }
}

/// A projekt PHP-fájljainak egyetlen, lusta és memoizált bejárása a PSR-4 gyökerek alatt: a 2. és 3. lépés (és a
/// `strict_types` mintavétel) osztozik rajta, így egy kérés — akárhány feloldandó névvel — legfeljebb egyszer járja be
/// a projektet.
pub struct PhpScan<'a> {
    fs: &'a dyn FileSystem,
    root: &'a Path,
    walk: MemoWalk<'a>,
    texts: RefCell<HashMap<PathBuf, Option<Rc<String>>>>,
}

impl<'a> PhpScan<'a> {
    pub fn new(fs: &'a dyn FileSystem, root: &'a Path, psr4: &Psr4) -> Self {
        let files = psr4
            .roots()
            .into_iter()
            .flat_map(move |dir| walk_php(fs, root.join(dir)));
        Self {
            fs,
            root,
            walk: MemoWalk::new(files),
            texts: RefCell::default(),
        }
    }

    /// A fájl szövege; kérésenként legfeljebb egyszer olvassa (az olvasási hiba is megjegyződik).
    pub fn read(&self, path: &Path) -> Option<Rc<String>> {
        self.texts
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| self.fs.read(path).ok().map(Rc::new))
            .clone()
    }

    /// A fájlok sorban (a már bejártak IO nélkül).
    pub fn files(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.walk.iter()
    }
}

/// 2. A projekt osztály-indexe: a PSR-4 gyökerek alatt a `Név.php` fájl, a névtere az útból.
pub struct ProjectClasses<'a, 's> {
    scan: &'a PhpScan<'s>,
    psr4: &'a Psr4,
}

impl<'a, 's> ProjectClasses<'a, 's> {
    pub fn new(scan: &'a PhpScan<'s>, psr4: &'a Psr4) -> Self {
        Self { scan, psr4 }
    }
}

impl NameResolver for ProjectClasses<'_, '_> {
    fn candidates(&self, short: &str) -> Vec<String> {
        let mut out = Vec::new();
        if self.psr4.is_empty() {
            return out;
        }
        let wanted = format!("{short}.php");
        let found = self.scan.files().filter(|path| {
            path.file_name()
                .is_some_and(|n| n.to_str() == Some(wanted.as_str()))
        });
        for path in found {
            if let Some(fqcn) =
                relative(self.scan.root, &path).and_then(|rel| self.psr4.fqcn_of(&rel))
            {
                push_unique(&mut out, fqcn);
            }
        }
        out
    }
}

/// Hány fájlt néz át legfeljebb a 3. lépés.
const USED_IMPORT_FILES: usize = 400;

/// A 3. lépés állapota: hány fájlt olvasott már el, és az ezekből összegyűjtött `alias → FQCN-ek` (előfordulási sorrendben).
#[derive(Default)]
struct UseIndex {
    next: usize,
    uses: HashMap<String, Vec<String>>,
}

/// A lánc 3. lépése: a projektben már használt importok, az első olyan `use X\Név;` (vagy `use X\Y as Név;`) sor.
/// Minden fájlt legfeljebb egyszer olvas el, a következő név a már összegyűjtött sorokból indul.
pub struct UsedImports<'a, 's> {
    scan: &'a PhpScan<'s>,
    index: RefCell<UseIndex>,
}

impl<'a, 's> UsedImports<'a, 's> {
    pub fn new(scan: &'a PhpScan<'s>) -> Self {
        Self {
            scan,
            index: RefCell::new(UseIndex::default()),
        }
    }
}

/// A fájl legfelső szintű `use` sorai: `(alias, FQCN)` párok.
fn file_uses(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    text.lines().filter_map(|line| {
        let rest = line.strip_prefix("use ")?.strip_suffix(';')?.trim();
        if rest.starts_with("function ") || rest.starts_with("const ") || rest.contains('{') {
            return None;
        }
        let (path, alias) = match rest.split_once(" as ") {
            Some((p, a)) => (p.trim(), a.trim()),
            None => (rest, rest.rsplit('\\').next().unwrap_or(rest)),
        };
        Some((alias.to_owned(), path.trim_start_matches('\\').to_owned()))
    })
}

impl NameResolver for UsedImports<'_, '_> {
    /// Az összes különböző `use X\Név;` cél (a fájlokat egyszer olvassa, legfeljebb `USED_IMPORT_FILES`-t).
    fn candidates(&self, short: &str) -> Vec<String> {
        let mut index = self.index.borrow_mut();
        while index.next < USED_IMPORT_FILES {
            let Some(path) = self.scan.walk.get(index.next) else {
                index.next = USED_IMPORT_FILES;
                break;
            };
            index.next += 1;
            let Some(text) = self.scan.read(&path) else {
                continue;
            };
            for (alias, fqcn) in file_uses(&text) {
                push_unique(index.uses.entry(alias).or_default(), fqcn);
            }
        }
        index.uses.get(short).cloned().unwrap_or_default()
    }
}

/// 4. A PHP globális osztályai / interfészei (névtérben `use X;` kell hozzájuk).
pub struct PhpGlobals;

const GLOBALS: &[&str] = &[
    "ArgumentCountError",
    "ArithmeticError",
    "ArrayAccess",
    "ArrayIterator",
    "ArrayObject",
    "BadFunctionCallException",
    "BadMethodCallException",
    "Closure",
    "Countable",
    "DateInterval",
    "DatePeriod",
    "DateTime",
    "DateTimeImmutable",
    "DateTimeInterface",
    "DateTimeZone",
    "DomainException",
    "Error",
    "ErrorException",
    "Exception",
    "Generator",
    "InvalidArgumentException",
    "Iterator",
    "IteratorAggregate",
    "JsonException",
    "JsonSerializable",
    "LengthException",
    "LogicException",
    "OutOfBoundsException",
    "OutOfRangeException",
    "OverflowException",
    "PDO",
    "PDOException",
    "RangeException",
    "ReflectionClass",
    "ReflectionException",
    "ReflectionMethod",
    "ReflectionNamedType",
    "ReflectionObject",
    "ReflectionProperty",
    "RuntimeException",
    "SplFileInfo",
    "SplObjectStorage",
    "SplQueue",
    "SplStack",
    "Stringable",
    "Throwable",
    "Traversable",
    "TypeError",
    "UnexpectedValueException",
    "UnhandledMatchError",
    "UnitEnum",
    "BackedEnum",
    "ValueError",
    "WeakMap",
];

impl NameResolver for PhpGlobals {
    fn candidates(&self, short: &str) -> Vec<String> {
        if GLOBALS.contains(&short) {
            vec![short.to_owned()]
        } else {
            Vec::new()
        }
    }
}

/// 5. A Laravel keretrendszer gyakori osztályai (a `vendor/` nincs beindexelve): csak Laravel-projektben, és csak
///    az egyértelmű rövid nevek (a `Response` / `Middleware` több csomagban is van, ezeket `use:`-zal kell megadni).
pub struct LaravelNames;

const LARAVEL: &[(&str, &str)] = &[
    ("Arr", "Illuminate\\Support\\Arr"),
    ("Auth", "Illuminate\\Support\\Facades\\Auth"),
    ("AssertableInertia", "Inertia\\Testing\\AssertableInertia"),
    (
        "BelongsTo",
        "Illuminate\\Database\\Eloquent\\Relations\\BelongsTo",
    ),
    (
        "BelongsToMany",
        "Illuminate\\Database\\Eloquent\\Relations\\BelongsToMany",
    ),
    ("Builder", "Illuminate\\Database\\Eloquent\\Builder"),
    ("Blueprint", "Illuminate\\Database\\Schema\\Blueprint"),
    ("Cache", "Illuminate\\Support\\Facades\\Cache"),
    ("Carbon", "Illuminate\\Support\\Carbon"),
    ("Collection", "Illuminate\\Support\\Collection"),
    ("DB", "Illuminate\\Support\\Facades\\DB"),
    (
        "Factory",
        "Illuminate\\Database\\Eloquent\\Factories\\Factory",
    ),
    ("FormRequest", "Illuminate\\Foundation\\Http\\FormRequest"),
    ("Gate", "Illuminate\\Support\\Facades\\Gate"),
    (
        "HasFactory",
        "Illuminate\\Database\\Eloquent\\Factories\\HasFactory",
    ),
    (
        "HasMany",
        "Illuminate\\Database\\Eloquent\\Relations\\HasMany",
    ),
    (
        "HasOne",
        "Illuminate\\Database\\Eloquent\\Relations\\HasOne",
    ),
    ("Hash", "Illuminate\\Support\\Facades\\Hash"),
    ("Inertia", "Inertia\\Inertia"),
    ("JsonResponse", "Illuminate\\Http\\JsonResponse"),
    ("Log", "Illuminate\\Support\\Facades\\Log"),
    ("Migration", "Illuminate\\Database\\Migrations\\Migration"),
    ("Model", "Illuminate\\Database\\Eloquent\\Model"),
    ("RedirectResponse", "Illuminate\\Http\\RedirectResponse"),
    (
        "RefreshDatabase",
        "Illuminate\\Foundation\\Testing\\RefreshDatabase",
    ),
    ("Request", "Illuminate\\Http\\Request"),
    ("Route", "Illuminate\\Support\\Facades\\Route"),
    ("Rule", "Illuminate\\Validation\\Rule"),
    ("Schema", "Illuminate\\Support\\Facades\\Schema"),
    ("Seeder", "Illuminate\\Database\\Seeder"),
    ("ServiceProvider", "Illuminate\\Support\\ServiceProvider"),
    ("SoftDeletes", "Illuminate\\Database\\Eloquent\\SoftDeletes"),
    ("Storage", "Illuminate\\Support\\Facades\\Storage"),
    ("Str", "Illuminate\\Support\\Str"),
    ("Validator", "Illuminate\\Support\\Facades\\Validator"),
    ("WithFaker", "Illuminate\\Foundation\\Testing\\WithFaker"),
];

impl NameResolver for LaravelNames {
    fn candidates(&self, short: &str) -> Vec<String> {
        LARAVEL
            .iter()
            .find(|(name, _)| *name == short)
            .map(|(_, fqcn)| (*fqcn).to_owned())
            .into_iter()
            .collect()
    }
}

/// A lánc: a tagok sorban, az első találatig.
pub struct ResolverChain<'a> {
    links: Vec<Box<dyn NameResolver + 'a>>,
}

impl<'a> ResolverChain<'a> {
    /// A PHP-lánc: explicit → projekt-index → használt importok → globálisok.
    pub fn php(
        explicit: Vec<(String, String)>,
        scan: &'a PhpScan<'_>,
        psr4: &'a Psr4,
        laravel: bool,
    ) -> Self {
        let mut links: Vec<Box<dyn NameResolver + 'a>> = vec![
            Box::new(ExplicitUse::new(explicit)),
            Box::new(ProjectClasses::new(scan, psr4)),
            Box::new(UsedImports::new(scan)),
            Box::new(PhpGlobals),
        ];
        if laravel {
            links.push(Box::new(LaravelNames));
        }
        Self { links }
    }

    /// A teljes név, vagy `None` (feloldatlan); több azonos nevű osztálynál az első.
    pub fn resolve(&self, short: &str) -> Option<String> {
        self.links.iter().find_map(|link| link.resolve(short))
    }

    /// A feloldás eredménye: a lánc első olyan tagja dönt, amelynek van jelöltje; ha az több különböző teljes nevet
    /// ismer (pl. `Response`: Illuminate / Inertia / Symfony), az eredmény `Ambiguous` — nem választunk csendben.
    /// A projekt saját osztálya (PSR-4) megelőzi a más fájlokból összegyűjtött importokat.
    pub fn resolve_all(&self, short: &str) -> Resolution {
        for link in &self.links {
            let mut found = link.candidates(short);
            match found.len() {
                0 => {}
                1 => return Resolution::One(found.remove(0)),
                _ => {
                    found.sort();
                    return Resolution::Ambiguous(found);
                }
            }
        }
        Resolution::Unknown
    }
}
