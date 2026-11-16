//! Strategy minta: a célnyelv egy `Emitter` megvalósítás (ma `PhpEmitter`), a tokenizáló / parser / AST érintetlen
//! marad, ha új nyelv érkezik. Az emitter sorokat ad egy lusta iterátoron át (`impl Iterator`), a hívó addig húzza,
//! amíg kell; a fájl összeállítását (névtér, importok, fejléc) a nyelv `Builder`-e végzi (PHP: `PhpFileBuilder`).

use std::collections::BTreeSet;

use super::ast::{Member, TypeDecl, TypeKind};
use crate::model::FileGraph;

/// A fájl fejléce: névtér, importok (FQCN, alias) és a nyelvi kapcsolók.
#[derive(Debug, Clone, Copy)]
pub struct FileSpec<'a> {
    pub namespace: Option<&'a str>,
    pub imports: &'a [(String, Option<String>)],
    pub strict_types: bool,
}

/// Egy célnyelv kódgenerátora.
pub trait Emitter {
    /// A generált fájlok kiterjesztése (pont nélkül).
    fn extension(&self) -> &'static str;

    /// Egy típus-deklaráció sorai (behúzás nélkül a legkülső szinten; a tagok egy szinttel beljebb).
    fn declaration<'a>(&'a self, decl: &'a TypeDecl) -> impl Iterator<Item = String> + 'a;

    /// Csak a tagok sorai (egy szint behúzással, tagonként egy üres sorral elválasztva): `--into` / `--replace`.
    /// `kind`: a befogadó típus fajtája (interfésznek nincs törzse).
    fn members<'a>(
        &'a self,
        kind: TypeKind,
        members: &'a [Member],
    ) -> impl Iterator<Item = String> + 'a;

    /// A kódban hivatkozott, importra szoruló osztály-nevek (típusok, ős, interfészek, törzsek `new X` / `X::`).
    /// A kifejezésben magában deklarált nevek kimaradnak.
    fn referenced_names(&self, decls: &[TypeDecl], loose: &[Member]) -> BTreeSet<String>;

    /// A teljes új fájl szövege a fejléccel és a deklarációkkal.
    fn file(&self, spec: &FileSpec<'_>, decls: &[TypeDecl]) -> String;

    /// Szintaxis-ellenőrzés a nyelv parserével: az első hiba (1-alapú sor, üzenet).
    fn check(&self, path: &str, text: &str) -> Result<(), (u32, String)>;

    /// A meglévő fájl váza (konténerek, metódus-tartományok, importok) a beszúráshoz / cseréhez.
    fn analyze(&self, path: &str, text: &str) -> FileGraph;
}
