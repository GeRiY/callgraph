//! A lekérdezés réteg-kapcsolói (`crate::profile::LayerScope`): a `--layer` szűrő a mappa-bejárásban (a kifejezetten
//! megnevezett fájl mindig marad), a `--by-layer` sorrend, és a `--layers` címkéhez az osztályozó (a profillal
//! azonos besorolás). Kapcsoló nélkül nincs hatásuk.

use std::path::Path;

use super::{QueryEngine, QueryResult};
use crate::error::{CallgraphError, Result};
use crate::profile::LayerScope;

impl QueryEngine {
    /// A réteg-kapcsolók bekapcsolása: `filter` vesszős réteg-lista (`service,repository`), `by_layer` a hívók
    /// rétegek szerinti sorrendje; ismeretlen rétegnévnél használati hiba az ismert nevekkel.
    /// A rétegek a profillal azonos besorolással: a projekt-gráffal (a teljes projekt frissítve, mint a `profile`-nál)
    /// a kód-jelet kívánó szerepek és az enum-fájlok a gráfból döntenek; gráf nélkül a profil tartalék-szabálya.
    pub fn with_layers(mut self, filter: Option<&str>, by_layer: bool) -> Result<Self> {
        let mut scope =
            LayerScope::new(&self.root, filter, by_layer).map_err(CallgraphError::Usage)?;
        if let Some(shared) = self.project_graph() {
            let mut graph = crate::graph::lock(&shared);
            let files = self.walk(std::slice::from_ref(&self.root));
            if graph.refresh(&files, true, &self.analyzer).is_ok() {
                let _ = graph.save();
                scope.settle_with(&files, &graph);
            }
        }
        self.layers = Some(scope);
        Ok(self)
    }

    /// A réteg-kapcsolók osztályozója (ha be vannak kapcsolva).
    pub(crate) fn layer_scope(&self) -> Option<&LayerScope> {
        self.layers.as_ref()
    }

    /// `--by-layer`: a sorok (fájlok) rétegek szerint, stabilan — a hívotthoz legközelebbi réteg elöl.
    pub fn sort_by_layer(&self, rows: &mut [QueryResult]) {
        if let Some(scope) = self.layers.as_ref().filter(|s| s.by_layer()) {
            rows.sort_by_key(|row| scope.order_key(&row.graph.file));
        }
    }

    /// A `--layer` szűrő átengedi-e a (gyökérhez relatív) fájlt; szűrő nélkül mindig.
    pub(super) fn keeps_layer(&self, rel: &Path) -> bool {
        self.layers.as_ref().is_none_or(|scope| scope.keeps(rel))
    }
}
