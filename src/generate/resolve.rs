//! Chain of Responsibility: a rövid osztálynév teljes névvé (FQCN) oldása láncolt feloldókkal, az első találatig:
//! 1. explicit `use:` (és a célfájl meglévő importjai) → 2. a projekt osztály-indexe (PSR-4 + fájlnév) →
//! 3. a projektben már használt importok (`use X\Y;` sorok) → 4. a PHP globális osztályai → feloldatlan.
//!
//! A 2. és 3. lépés lustán halad: a jelölteket (fájlokat) folyamatosan adja, és az első találatnál megáll.

use std::path::Path;

use super::fs::{FileSystem, walk_php};
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

/// 2. A projekt osztály-indexe: a PSR-4 gyökerek alatt a `Név.php` fájl, a névtere az útból.
pub struct ProjectClasses<'a> {
    fs: &'a dyn FileSystem,
    root: &'a Path,
    psr4: &'a Psr4,
}

impl<'a> ProjectClasses<'a> {
    pub fn new(fs: &'a dyn FileSystem, root: &'a Path, psr4: &'a Psr4) -> Self {
        Self { fs, root, psr4 }
    }
}

impl NameResolver for ProjectClasses<'_> {
    fn resolve(&self, short: &str) -> Option<String> {
        if self.psr4.is_empty() {
            return None;
        }
        let wanted = format!("{short}.php");
        self.psr4
            .roots()
            .into_iter()
            .flat_map(|dir| walk_php(self.fs, self.root.join(dir)))
            .find(|path| {
                path.file_name()
                    .is_some_and(|n| n.to_str() == Some(wanted.as_str()))
            })
            .and_then(|path| relative(self.root, &path))
            .and_then(|rel| self.psr4.fqcn_of(&rel))
    }
}

/// Hány fájlt néz át legfeljebb a 3. lépés egy névnél.
const USED_IMPORT_FILES: usize = 400;

/// 3. A projektben már használt importok: az első olyan `use X\Név;` (vagy `use X\Y as Név;`) sor.
pub struct UsedImports<'a> {
    fs: &'a dyn FileSystem,
    root: &'a Path,
    psr4: &'a Psr4,
}

impl<'a> UsedImports<'a> {
    pub fn new(fs: &'a dyn FileSystem, root: &'a Path, psr4: &'a Psr4) -> Self {
        Self { fs, root, psr4 }
    }
}

/// A fájl legfelső szintű `use` sorai közül az, amelyik a `short` nevet adja.
fn find_use(text: &str, short: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix("use ")?.strip_suffix(';')?.trim();
        if rest.starts_with("function ") || rest.starts_with("const ") || rest.contains('{') {
            return None;
        }
        let (path, alias) = match rest.split_once(" as ") {
            Some((p, a)) => (p.trim(), a.trim()),
            None => (rest, rest.rsplit('\\').next().unwrap_or(rest)),
        };
        (alias == short).then(|| path.trim_start_matches('\\').to_owned())
    })
}

impl NameResolver for UsedImports<'_> {
    fn resolve(&self, short: &str) -> Option<String> {
        self.psr4
            .roots()
            .into_iter()
            .flat_map(|dir| walk_php(self.fs, self.root.join(dir)))
            .take(USED_IMPORT_FILES)
            .filter_map(|path| self.fs.read(&path).ok())
            .find_map(|text| find_use(&text, short))
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
    pub fn php(
        explicit: Vec<(String, String)>,
        fs: &'a dyn FileSystem,
        root: &'a Path,
        psr4: &'a Psr4,
    ) -> Self {
        Self {
            links: vec![
                Box::new(ExplicitUse::new(explicit)),
                Box::new(ProjectClasses::new(fs, root, psr4)),
                Box::new(UsedImports::new(fs, root, psr4)),
                Box::new(PhpGlobals),
            ],
        }
    }

    /// A teljes név, vagy `None` (feloldatlan).
    pub fn resolve(&self, short: &str) -> Option<String> {
        self.links.iter().find_map(|link| link.resolve(short))
    }
}
