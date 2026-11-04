//! Közös container/caller-követés a nikic `currentContainer` / `currentMethod` szerint:
//! osztály-kilépés `None`-ra állít (névtelen osztály után is), metódus-kilépés a callert törli,
//! a closure-mélység a metódus-belépéskor kerül a névbe, a `new class(...)` argumentumai
//! pedig a törzs UTÁN járt (container `None`).

use mago_span::HasSpan;
use mago_syntax::cst::{AnonymousClass, ClassLikeMember};

/// A névtelen osztály belépési keretei (az argumentumlista kiértékelési kontextusához).
#[derive(Default)]
pub(super) struct AnonFrames {
    frames: Vec<Frame>,
}

struct Frame {
    args_start: Option<u32>,
    caller_after: Option<String>,
    saved: Option<(Option<String>, Option<String>)>,
}

impl AnonFrames {
    /// Belépés: a container `<anonymous>`; a törzs utáni caller `None`, ha van benne metódus.
    pub(super) fn enter(
        &mut self,
        node: &AnonymousClass<'_>,
        container: &mut Option<String>,
        caller: &Option<String>,
    ) {
        let has_method = node
            .members
            .iter()
            .any(|m| matches!(m, ClassLikeMember::Method(_)));
        self.frames.push(Frame {
            args_start: node.argument_list.as_ref().map(|a| a.span().start.offset),
            caller_after: if has_method { None } else { caller.clone() },
            saved: None,
        });
        *container = Some("<anonymous>".into());
    }

    /// Az argumentumlista elején: ha a legfelső névtelen osztályé, a törzs utáni kontextus.
    pub(super) fn args_in(
        &mut self,
        start: u32,
        container: &mut Option<String>,
        caller: &mut Option<String>,
    ) {
        if let Some(f) = self.frames.last_mut()
            && f.args_start == Some(start)
            && f.saved.is_none()
        {
            f.saved = Some((container.take(), caller.take()));
            *caller = f.caller_after.clone();
        }
    }

    pub(super) fn args_out(
        &mut self,
        start: u32,
        container: &mut Option<String>,
        caller: &mut Option<String>,
    ) {
        if let Some(f) = self.frames.last_mut()
            && f.args_start == Some(start)
            && let Some((c, k)) = f.saved.take()
        {
            *container = c;
            *caller = k;
        }
    }

    /// Kilépés: a container `None` (a külső osztályé sem él tovább).
    pub(super) fn leave(&mut self, container: &mut Option<String>) {
        self.frames.pop();
        *container = None;
    }
}

/// Container/caller/closure-mélység a vezérlés-visitorokhoz.
#[derive(Default)]
pub(super) struct Ctx {
    pub(super) container: Option<String>,
    pub(super) caller: Option<String>,
    depth: usize,
    anon: AnonFrames,
}

impl Ctx {
    pub(super) fn enter_class(&mut self, name: &[u8]) {
        self.container = Some(String::from_utf8_lossy(name).into_owned());
    }
    pub(super) fn leave_class(&mut self) {
        self.container = None;
    }
    pub(super) fn enter_anon(&mut self, node: &AnonymousClass<'_>) {
        self.anon.enter(node, &mut self.container, &self.caller);
    }
    pub(super) fn leave_anon(&mut self) {
        self.anon.leave(&mut self.container);
    }
    pub(super) fn args_in(&mut self, start: u32) {
        self.anon
            .args_in(start, &mut self.container, &mut self.caller);
    }
    pub(super) fn args_out(&mut self, start: u32) {
        self.anon
            .args_out(start, &mut self.container, &mut self.caller);
    }
    pub(super) fn enter_method(&mut self, name: &[u8]) {
        let name = String::from_utf8_lossy(name).into_owned();
        self.caller = Some(if self.depth > 0 {
            format!("{name}.<closure>{}", self.depth)
        } else {
            name
        });
    }
    pub(super) fn leave_method(&mut self) {
        self.caller = None;
    }
    pub(super) fn enter_closure(&mut self) {
        self.depth += 1;
    }
    pub(super) fn leave_closure(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
    /// A sor caller-neve (`__top__`, ha nincs metódus).
    pub(super) fn caller_name(&self) -> String {
        self.caller.clone().unwrap_or_else(|| "__top__".into())
    }
}
