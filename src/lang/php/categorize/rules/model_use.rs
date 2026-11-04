//! Modellnévtérből érkező hívások.

use crate::lang::php::categorize::{CallInfo, CategoryRule};

/// Modell-hívások felismerése (framework-stratégia ismert modell-ősei alapján).
#[derive(Debug, Default, Clone, Copy)]
pub struct ModelUseRule;

impl CategoryRule for ModelUseRule {
    fn categorize(&self, call: &CallInfo<'_>) -> Option<&'static str> {
        let resolved = call.resolved?.to_ascii_lowercase();
        (resolved.contains("models\\")
            || call.framework.model_bases().iter().any(|base| {
                let base = base.to_ascii_lowercase();
                resolved == base
                    || resolved.starts_with(&(base.clone() + "\\"))
                    || resolved.starts_with(&(base + "::"))
            }))
        .then_some("model_use")
    }
}
