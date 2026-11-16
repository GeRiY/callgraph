//! Blade-frontend (`.blade.php`): a Laravel sablon-nyelve külön nyelv, nem PHP. Fájlonként egy `view` container
//! (neve a `resources/views`-hoz relatív pontozott nézet-név: `admin/users/index.blade.php` → `admin.users.index`),
//! a `@section` / `@push` / `@slot` / `<x-slot:név>` blokkok metódusként sortartománnyal (`section:content`),
//! a nézet-használat (`@extends`, `@include`, `@each`, `<x-alert>`, `<livewire:x>`) hívásként a cél fájlra
//! feloldva, a `{{ }}` / `{!! !!}` / `@php` / direktíva-argumentum PHP-kifejezései a PHP-elemzővel (sorhelyesen),
//! a `@if` / `@foreach` / `@auth` / `@can` vezérlési ágként, a `@props` property-ként. A `route('név')` és a
//! `__('kulcs')` / `@lang('kulcs')` hívás `resolved`-je `route::név` / `i18n::kulcs`: a Laravel route-tábla és az
//! i18n-kereszthivatkozás ezt olvassa. Nem teljes Blade-fordító: a hibás sablon is elkészül, ami nem ismerhető fel,
//! kimarad.

mod build;
mod php;
mod scan;
mod view;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};

pub use build::BLOCK_KIND;
pub(crate) use view::{normalize, view_name};

/// A `.blade.php` fájlok nyelve. A kiterjesztése a PHP-é (`php`), ezért a registry külön, a fájlnév végére
/// (`.blade.php`) illeszt, és a PHP elé teszi.
#[derive(Debug, Default, Clone, Copy)]
pub struct BladeFrontend;

impl BladeFrontend {
    /// Alapértelmezett összeállítás.
    pub fn with_defaults() -> Self {
        Self
    }
}

/// Blade-sablon-e az út (kis-/nagybetűtől függetlenül).
pub fn is_blade_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".blade.php"))
}

impl Language for BladeFrontend {
    fn id(&self) -> &'static str {
        "blade"
    }
    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }
    fn extensions(&self) -> &'static [&'static str] {
        // a `php`-t a PHP-backend adja; a Blade a `registry::for_path`-ban a fájlnév-végre illeszt
        &[]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        Ok(Analysis {
            graph: build::build(&file.path_string(), &text),
            warnings: Vec::new(),
        })
    }
}

impl LanguageFrontend for BladeFrontend {
    /// A nézet-név (`layouts.app`) fájlja a hivatkozó fájl projektjében, ha létezik.
    fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf> {
        let rel = view::view_file(spec)?;
        let from = from.to_string_lossy().replace('\\', "/");
        let root = &from[..from.rfind("resources/views/")?];
        let path = Path::new(root).join(rel);
        path.is_file().then_some(path)
    }
}

/// Egy nézet-hivatkozás a sablonban (a keresztreferenciához: „ki használja ezt a nézetet”).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewRef {
    /// A hivatkozott nézet pontozott neve (`layouts.app`, `components.alert`).
    pub view: String,
    pub line: u32,
    /// `@extends`, `@include`, `@each`, `@component`, `<x-…>`.
    pub how: &'static str,
}

/// A sablon nézet-hivatkozásai (a `@extends` / `@include*` / `@each` / `@component` / `<x-…>` célja).
pub(crate) fn view_refs(text: &str) -> Vec<ViewRef> {
    let lines = crate::core::LineIndex::new(text);
    let mut out = Vec::new();
    for item in scan::scan(text) {
        match item {
            scan::Item::Directive {
                name,
                args: Some(args),
                start,
                ..
            } => {
                let parts = scan::split_top(&args, b',');
                let literal =
                    |index: usize| parts.get(index).and_then(|(part, _)| scan::literal(part));
                let (how, views): (&'static str, Vec<String>) =
                    match name.to_ascii_lowercase().as_str() {
                        "extends" => ("@extends", literal(0).into_iter().collect()),
                        "include" | "includeif" | "each" => {
                            let how = if name.eq_ignore_ascii_case("each") {
                                "@each"
                            } else {
                                "@include"
                            };
                            (how, literal(0).into_iter().collect())
                        }
                        "includewhen" | "includeunless" => {
                            ("@include", literal(1).into_iter().collect())
                        }
                        "component" => ("@component", literal(0).into_iter().collect()),
                        _ => continue,
                    };
                for view in views {
                    out.push(ViewRef {
                        view: view::normalize(&view),
                        line: lines.line_of(start),
                        how,
                    });
                }
            }
            scan::Item::Tag {
                name,
                start,
                close: false,
                ..
            } if name.starts_with("x-") && !name.starts_with("x-slot") && !name.contains("::") => {
                out.push(ViewRef {
                    view: format!("components.{}", &name[2..]),
                    line: lines.line_of(start),
                    how: "<x-…>",
                });
            }
            _ => {}
        }
    }
    out
}
