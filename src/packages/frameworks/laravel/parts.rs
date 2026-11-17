//! A `Laravel` homlokzat részei (R5): a route-tábla, a route-hivatkozások, az űrlap-szerződések és a modell-katalógus
//! külön típusban. A `Laravel` a gyökeret és a lemezes cache-t tartja, és lustán (a `scan*` hívásokra) tölti a
//! részeket; a publikus metódusok nevei változatlanok.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Reference;
use super::forms::Form;
use super::models::ModelData;
use super::routes::Route;
use crate::core::diag::Diagnostics;

/// A route-tábla: a route-ok, a név-index és a betöltöttség (a dinamikus nevek, a be nem töltött route-fájlok).
/// A lemezes cache ezt tárolja (a `by_name` nélkül: az a route-okból épül újra).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct RouteTable {
    pub routes: Vec<Route>,
    /// Név → a `routes` indexe (az utolsó definíció nyer, mint a Laravelben).
    #[serde(skip)]
    pub by_name: HashMap<String, usize>,
    /// A dinamikus nevű route-ok helye (`->name($name)` egy ciklusban): ilyenkor az „ismeretlen név” nem biztos.
    pub dynamic_names: Vec<(String, u32)>,
    /// A `routes/` alatti, de sehonnan be nem töltött fájlok (se `withRouting`, se `require`): a route-jaik
    /// nincsenek a táblában.
    pub unloaded: Vec<String>,
}

impl RouteTable {
    /// A név → route index újraépítése (az utolsó definíció nyer).
    pub(super) fn rebuild_names(&mut self) {
        self.by_name.clear();
        for (index, route) in self.routes.iter().enumerate() {
            if let Some(name) = &route.name {
                self.by_name.insert(name.clone(), index);
            }
        }
    }
}

/// A `route(..)` hivatkozások a projektben (csak a `references` kéréssel végzett `scan` után).
#[derive(Debug, Clone, Default)]
pub(super) struct RouteRefs {
    items: Vec<Reference>,
}

impl RouteRefs {
    pub(super) fn push(&mut self, reference: Reference) {
        self.items.push(reference);
    }

    pub(super) fn iter(&self) -> std::slice::Iter<'_, Reference> {
        self.items.iter()
    }
}

/// Az ellenőrzött `useForm` küldések (csak a `scan_forms` után), a kihagyott elemek és a PSR-4 leképezés.
#[derive(Debug, Clone, Default)]
pub(super) struct FormContracts {
    pub items: Vec<Form>,
    /// A kihagyott (nem értelmezhető) elemek: a nézetek lábléce összesíti (`forms.rs`).
    pub diagnostics: Diagnostics,
    /// A `composer.json` PSR-4 leképezése (előtag, mappa) — csak a célzott `scan_forms_for` után; a teljes
    /// projekt-keresésben üres (ott a gráfokból gyűjtött osztály-térkép van).
    pub psr4: Vec<(String, String)>,
}

/// Az Eloquent-modellek és a migrációk sémája (csak a `scan_models` után).
#[derive(Debug, Clone, Default)]
pub(super) struct ModelCatalog {
    pub data: Option<ModelData>,
}
