//! The resource key in the graph: a resource class becomes a `framework_use` call with the target
//! `filament::resource:<Model>`, so `--grep filament::resource:Post` (or just `--grep Post` over the Filament
//! directory) finds the resource from the model side. Called by the PHP frontend while the file graph is built; files
//! that are no Filament resource get nothing.

use super::scan::{self, Found};
use crate::model::Call;

/// The call of the resource key of a PHP file.
pub(crate) fn php_calls(rel: &str, text: &str) -> Vec<Call> {
    if !text.contains("Filament\\Resources") {
        return Vec::new();
    }
    let Some(Found::Resource(resource)) = scan::scan_file(rel, text) else {
        return Vec::new();
    };
    let Some(model) = resource.model.as_deref() else {
        return Vec::new();
    };
    let key = format!("filament::resource:{model}");
    vec![
        Call::new("resource", key.clone(), "framework_use")
            .at(resource.model_line)
            .within(Some(resource.name.clone()), None)
            .resolved(Some(key)),
    ]
}
