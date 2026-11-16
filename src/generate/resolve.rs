//! Chain of Responsibility: a rövid osztálynév teljes névvé (FQCN) oldása láncolt feloldókkal, az első találatig:
//! 1. explicit `use:` (és a célfájl meglévő importjai) → 2. a projekt osztály-indexe (PSR-4 + fájlnév) →
//! 3. a projektben már használt importok (`use X\Y;` sorok) → 4. a PHP globális osztályai → feloldatlan.
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
    /// A rövid névhez a teljes név, ha ez a tag tudja.
    fn resolve(&self, short: &str) -> Option<String>;
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
    fn resolve(&self, short: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(alias, _)| alias == short)
            .map(|(_, fqcn)| fqcn.clone())
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
    fn resolve(&self, short: &str) -> Option<String> {
        if self.psr4.is_empty() {
            return None;
        }
        let wanted = format!("{short}.php");
        self.scan
            .files()
            .find(|path| {
                path.file_name()
                    .is_some_and(|n| n.to_str() == Some(wanted.as_str()))
            })
            .and_then(|path| relative(self.scan.root, &path))
            .and_then(|rel| self.psr4.fqcn_of(&rel))
    }
}

/// Hány fájlt néz át legfeljebb a 3. lépés.
const USED_IMPORT_FILES: usize = 400;

/// A 3. lépés állapota: hány fájlt olvasott már el, és az ezekből összegyűjtött `alias → FQCN` párok (az első nyer).
#[derive(Default)]
struct UseIndex {
    next: usize,
    uses: HashMap<String, String>,
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
    fn resolve(&self, short: &str) -> Option<String> {
        let mut index = self.index.borrow_mut();
        loop {
            if let Some(found) = index.uses.get(short) {
                return Some(found.clone());
            }
            if index.next >= USED_IMPORT_FILES {
                return None;
            }
            let path = self.scan.walk.get(index.next)?;
            index.next += 1;
            let Some(text) = self.scan.read(&path) else {
                continue;
            };
            for (alias, fqcn) in file_uses(&text) {
                index.uses.entry(alias).or_insert(fqcn);
            }
        }
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
    fn resolve(&self, short: &str) -> Option<String> {
        GLOBALS.contains(&short).then(|| short.to_owned())
    }
}

/// A lánc: a tagok sorban, az első találatig.
pub struct ResolverChain<'a> {
    links: Vec<Box<dyn NameResolver + 'a>>,
}

impl<'a> ResolverChain<'a> {
    /// A PHP-lánc: explicit → projekt-index → használt importok → globálisok.
    pub fn php(explicit: Vec<(String, String)>, scan: &'a PhpScan<'_>, psr4: &'a Psr4) -> Self {
        Self {
            links: vec![
                Box::new(ExplicitUse::new(explicit)),
                Box::new(ProjectClasses::new(scan, psr4)),
                Box::new(UsedImports::new(scan)),
                Box::new(PhpGlobals),
            ],
        }
    }

    /// A teljes név, vagy `None` (feloldatlan).
    pub fn resolve(&self, short: &str) -> Option<String> {
        self.links.iter().find_map(|link| link.resolve(short))
    }
}
