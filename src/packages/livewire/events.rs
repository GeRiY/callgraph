//! The Livewire event key in the graph: every event name a component dispatches or listens to becomes a
//! `framework_use` call with the target `livewire::event:<name>`, so `--grep livewire::event:saved` (or just the
//! event name) finds the dispatcher and the listener in one go. Called by the PHP and the Blade frontend while the
//! file graph is built; files that are no Livewire component / template get nothing.

use super::scan;
use crate::model::Call;

/// The call of an event key.
fn event_call(line: u32, container: &str, caller: &str, name: &str) -> Call {
    let key = format!("livewire::event:{name}");
    Call::new("event", key.clone(), "framework_use")
        .at(line)
        .within(Some(container.to_owned()), Some(caller.to_owned()))
        .resolved(Some(key))
}

/// The event calls of a PHP file: the dispatch sites and the listener methods of a Livewire component class.
pub(crate) fn php_event_calls(rel: &str, text: &str) -> Vec<Call> {
    let Some(class) = scan::scan_class(rel, text) else {
        return Vec::new();
    };
    class
        .dispatches
        .iter()
        .chain(&class.listens)
        .filter_map(|site| {
            let method = site.method.as_deref()?;
            Some(event_call(site.line, &class.name, method, &site.name))
        })
        .collect()
}

/// The event calls of a Blade template (`$dispatch('x')`, `Livewire.on('x')`); `owner_of` is the block of a line.
pub(crate) fn blade_event_calls(
    text: &str,
    container: &str,
    owner_of: &dyn Fn(u32) -> String,
) -> Vec<Call> {
    if !scan::mentions_livewire(text) {
        return Vec::new();
    }
    let (dispatches, listens, _) = scan::events_of_text(text);
    dispatches
        .iter()
        .chain(&listens)
        .map(|(name, line)| event_call(*line, container, &owner_of(*line), name))
        .collect()
}
