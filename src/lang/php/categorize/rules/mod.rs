//! A kategória-szabályok (egy fájl = egy szabály).

mod component_use;
mod dynamic_call;
mod external_call;
mod framework_use;
mod global_helper;
mod helper_use;
mod i18n_helper;
mod instantiation;
mod internal_helper;
mod model_use;
mod native_php;
mod orm_relation;
mod static_call;

pub use component_use::ComponentUseRule;
pub use dynamic_call::DynamicCallRule;
pub use external_call::ExternalCallRule;
pub use framework_use::FrameworkUseRule;
pub use global_helper::GlobalHelperRule;
pub use helper_use::HelperUseRule;
pub use i18n_helper::I18nHelperRule;
pub use instantiation::InstantiationRule;
pub use internal_helper::InternalHelperRule;
pub use model_use::ModelUseRule;
pub use native_php::NativePhpRule;
pub use orm_relation::OrmRelationRule;
pub use static_call::StaticCallRule;

use super::CategoryRule;

/// Az alapértelmezett sorrend (a konkrétabb szabály elöl).
pub fn default_rules() -> Vec<Box<dyn CategoryRule>> {
    vec![
        Box::new(DynamicCallRule),
        Box::new(OrmRelationRule),
        Box::new(InternalHelperRule),
        Box::new(I18nHelperRule),
        Box::new(FrameworkUseRule),
        Box::new(ModelUseRule),
        Box::new(ComponentUseRule),
        Box::new(HelperUseRule),
        Box::new(NativePhpRule),
        Box::new(GlobalHelperRule),
        Box::new(InstantiationRule),
        Box::new(StaticCallRule),
        Box::new(ExternalCallRule),
    ]
}

#[cfg(test)]
mod tests;
