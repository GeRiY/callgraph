//! Nézet-nevek és fájlutak: `admin/users/index.blade.php` ↔ `admin.users.index`, a `<x-forms.input>` komponens
//! fájlja (anonim: `resources/views/components/forms/input.blade.php`, osztályos: `app/View/Components/Forms/
//! Input.php`), a `<livewire:user-table>` osztálya.

use std::path::Path;

/// A nézet-gyökér a fájl útjában.
const VIEWS: &str = "resources/views/";

/// A fájl útja `/`-rel, a `.blade.php` nélkül, a nézet-gyökértől: `admin/users/index`. A gyökér az útban utolsóként
/// előforduló `resources/views/` (különben a `views/`), ha nincs ilyen, az egész út (a fájlnév marad).
fn relative_stem(path: &str) -> String {
    let path = path.replace('\\', "/");
    let rest = match path.rfind(VIEWS) {
        Some(at) => &path[at + VIEWS.len()..],
        None => match path.rfind("/views/") {
            Some(at) => &path[at + "/views/".len()..],
            None => path.rsplit('/').next().unwrap_or(&path),
        },
    };
    let lower = rest.to_ascii_lowercase();
    let cut = if lower.ends_with(".blade.php") {
        rest.len() - ".blade.php".len()
    } else {
        rest.len()
    };
    rest[..cut].to_owned()
}

/// A fájl nézet-neve: `admin/users/index.blade.php` → `admin.users.index`.
pub(crate) fn view_name(path: &str) -> String {
    relative_stem(path).replace('/', ".")
}

/// A hivatkozott nézet-név normalizált alakja (`admin/users`, `admin.users`, `admin.users.blade` → `admin.users`);
/// a csomag-névterű (`mail::message`) marad.
pub(crate) fn normalize(name: &str) -> String {
    let name = name.trim().replace('/', ".");
    name.strip_suffix(".blade.php").unwrap_or(&name).to_owned()
}

/// A nézet-név fájlja a projekt gyökeréhez relatívan (`resources/views/layouts/app.blade.php`); csomag-névterű
/// (`mail::x`) névre `None`.
pub(crate) fn view_file(name: &str) -> Option<String> {
    let name = normalize(name);
    if name.is_empty() || name.contains("::") {
        return None;
    }
    Some(format!("{VIEWS}{}.blade.php", name.replace('.', "/")))
}

/// A hivatkozó fájl projekt-gyökere (a `resources/views/` előtti rész), ha az út tartalmazza.
fn project_root(from: &str) -> Option<String> {
    let from = from.replace('\\', "/");
    let at = from.rfind(VIEWS)?;
    Some(from[..at].to_owned())
}

/// Létezik-e a projekt-gyökérhez relatív fájl a lemezen (a gyökér ismeretlen vagy nincs lemezen: `false`).
fn exists(root: Option<&str>, rel: &str) -> bool {
    root.is_some_and(|root| Path::new(root).join(rel).is_file())
}

/// `user-table` → `UserTable` (a Laravel osztály-névvé alakítása).
fn studly(segment: &str) -> String {
    segment
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// A `<x-név>` komponens fájlja a gyökérhez relatívan: az anonim sablon (`components/név.blade.php`, vagy
/// `components/név/index.blade.php`), ha a lemezen az osztályos (`app/View/Components/Név.php`) létezik, az; ha
/// egyik sem látszik, az anonim sablon. Csomag-névterű (`x-pkg::name`) és dinamikus komponensre `None`.
pub(crate) fn component_file(name: &str, from: &str) -> Option<String> {
    let name = name.strip_prefix("x-")?;
    if name.contains("::") || name == "dynamic-component" || name.is_empty() {
        return None;
    }
    let root = project_root(from);
    let dir = name.replace('.', "/");
    let anonymous = format!("{VIEWS}components/{dir}.blade.php");
    let index = format!("{VIEWS}components/{dir}/index.blade.php");
    let class = format!(
        "app/View/Components/{}.php",
        name.split('.').map(studly).collect::<Vec<_>>().join("/")
    );
    let root = root.as_deref();
    Some(if exists(root, &anonymous) {
        anonymous
    } else if exists(root, &index) {
        index
    } else if exists(root, &class) {
        class
    } else {
        anonymous
    })
}

/// A `<livewire:név>` osztálya a gyökérhez relatívan, ha a lemezen megtalálható (`app/Livewire/Név.php`, régebben
/// `app/Http/Livewire/Név.php`); különben `None`.
pub(crate) fn livewire_file(name: &str, from: &str) -> Option<String> {
    let name = name.strip_prefix("livewire:")?;
    let root = project_root(from);
    let class = name.split('.').map(studly).collect::<Vec<_>>().join("/");
    ["app/Livewire", "app/Http/Livewire"]
        .iter()
        .map(|base| format!("{base}/{class}.php"))
        .find(|rel| exists(root.as_deref(), rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_names_are_dotted_and_relative_to_the_views_root() {
        assert_eq!(
            view_name("/p/resources/views/admin/users/index.blade.php"),
            "admin.users.index"
        );
        assert_eq!(view_name("resources\\views\\welcome.blade.php"), "welcome");
        assert_eq!(view_name("page.blade.php"), "page");
        assert_eq!(view_name("/p/app/views/mail/x.blade.php"), "mail.x");
    }

    #[test]
    fn references_resolve_to_view_files() {
        assert_eq!(
            view_file("layouts.app").as_deref(),
            Some("resources/views/layouts/app.blade.php")
        );
        assert_eq!(
            view_file("admin/users").as_deref(),
            Some("resources/views/admin/users.blade.php")
        );
        assert_eq!(view_file("mail::message"), None);
        assert_eq!(normalize("a/b"), "a.b");
    }

    #[test]
    fn components_resolve_to_anonymous_templates_by_default() {
        assert_eq!(
            component_file("x-forms.input", "/nope/resources/views/a.blade.php").as_deref(),
            Some("resources/views/components/forms/input.blade.php")
        );
        assert_eq!(component_file("x-pkg::alert", "a.blade.php"), None);
        assert_eq!(studly("user-table"), "UserTable");
        assert_eq!(livewire_file("livewire:counter", "a.blade.php"), None);
    }
}
