use super::super::context::AnonFrames;
use crate::lang::php::{
    analysis::{NameResolver, type_tracker::TypeMap},
    categorize::{CallInfo, CategoryChain},
    framework::{FrameworkStrategy, GenericStrategy},
};
use crate::model::Call;
use std::{collections::HashSet, rc::Rc};

pub(super) struct State {
    pub(super) container: Option<String>,
    pub(super) caller: Option<String>,
    pub(super) closure_depth: usize,
    pub(super) anon: AnonFrames,
    pub(super) resolver: NameResolver,
    pub(super) tracker: TypeMap,
    pub(super) imports: HashSet<String>,
    pub(super) framework: Rc<dyn FrameworkStrategy>,
    pub(super) chain: CategoryChain,
    pub(super) calls: Vec<Call>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            container: None,
            caller: None,
            closure_depth: 0,
            anon: AnonFrames::default(),
            resolver: NameResolver::default(),
            tracker: TypeMap::default(),
            imports: HashSet::new(),
            framework: Rc::new(GenericStrategy),
            chain: CategoryChain::with_defaults(),
            calls: Vec::new(),
        }
    }
}

pub(super) fn categorize(call: &mut Call, state: &State) {
    let short = call
        .target
        .split("::")
        .next()
        .filter(|x| x.contains('\\') || x.chars().next().is_some_and(char::is_uppercase));
    let info = CallInfo {
        kind: &call.kind,
        target: &call.target,
        on: call.on.as_deref(),
        resolved: call.resolved.as_deref(),
        class_short_name: short,
        has_use_import: short.is_some_and(|x| state.imports.contains(x)),
        is_this: call.on.as_deref() == Some("this"),
        framework: state.framework.as_ref(),
    };
    call.category = state.chain.categorize(&info).into();
}
