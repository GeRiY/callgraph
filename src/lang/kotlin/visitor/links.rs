//! DI- és navigációs élek, amelyek nem puszta hívások: `@Inject constructor(..)` / `@Inject lateinit var` fogyasztó
//! élek (`inject`), `@Provides` / `@Binds` szolgáltató élek (`provide`) és a `navController.navigate(..)` →
//! képernyő él (`nav`).

use tree_sitter::Node;

use super::super::known::{NATIVE_TYPES, is_in};
use super::super::parser::line;
use super::nav::{match_route, navigate_target};
use super::scan::ctor_params;
use super::util::{annotation_names, base_type, name_and_type};
use super::{KotlinVisitor, Scope};
use crate::model::{Call, FileGraphBuilder};

/// A befecskendezett típus neve a típus-szövegből: `Lazy<Repo>` / `Provider<Repo>` → `Repo`; a nyelvi és könyvtári
/// alaptípusok (`String`, `List`…) nem DI-típusok.
fn injected_type(text: &str) -> Option<String> {
    let base = base_type(text)?;
    let ty = if matches!(base.as_str(), "Lazy" | "Provider") {
        let inner = text.split_once('<')?.1;
        base_type(inner.trim_end_matches(['>', '?']))?
    } else {
        base
    };
    (!is_in(NATIVE_TYPES, &ty)).then_some(ty)
}

impl KotlinVisitor<'_, '_> {
    /// Egy DI-él a `caller` metódusból (`kind`: `inject` / `provide`), a célja `@Annotáció(Típus)` alakú.
    fn di_edge(
        &self,
        kind: &str,
        target: String,
        at: u32,
        container: Option<&str>,
        caller: &str,
        b: &mut FileGraphBuilder,
    ) {
        b.call(Call {
            container: container.map(str::to_string),
            caller: Some(caller.to_string()),
            line: at,
            end_line: None,
            kind: kind.to_string(),
            target,
            resolved: None,
            on: None,
            category: "framework_use".to_string(),
            args: None,
            handler: None,
            from: None,
        });
    }

    /// `@Inject constructor(private val repo: TodoRepository)`: minden paraméter-típus befecskendezett függőség
    /// (`inject` él a konstruktorból a típusra).
    pub(super) fn constructor_injection(
        &self,
        class: Node<'_>,
        ctor: Node<'_>,
        container: &str,
        b: &mut FileGraphBuilder,
    ) {
        let injected = annotation_names(self.parsed, ctor)
            .iter()
            .any(|a| matches!(a.as_str(), "Inject" | "AssistedInject"));
        if !injected {
            return;
        }
        for param in ctor_params(class) {
            if let (_, Some(ty)) = name_and_type(self.parsed, param)
                && let Some(ty) = injected_type(&ty)
            {
                self.di_edge(
                    "inject",
                    format!("@Inject({ty})"),
                    line(param),
                    Some(container),
                    "constructor",
                    b,
                );
            }
        }
    }

    /// `@Inject lateinit var repo: TodoRepository`: mező-befecskendezés (`inject` él az osztály-szintről).
    pub(super) fn field_injection(
        &self,
        property: Node<'_>,
        container: Option<&str>,
        b: &mut FileGraphBuilder,
    ) {
        if !annotation_names(self.parsed, property)
            .iter()
            .any(|a| a == "Inject")
        {
            return;
        }
        if let (_, Some(ty)) = name_and_type(self.parsed, property)
            && let Some(ty) = injected_type(&ty)
        {
            self.di_edge(
                "inject",
                format!("@Inject({ty})"),
                line(property),
                container,
                "__top__",
                b,
            );
        }
    }

    /// `@Provides fun provideRepo(..): TodoRepository` / `@Binds abstract fun bind(impl: RepoImpl): Repo`: a
    /// szolgáltató metódus a visszatérési típus forrása (`provide` él; a `@Binds` célja `Repo <- RepoImpl`).
    pub(super) fn provider_edge(
        &self,
        function: Node<'_>,
        container: Option<&str>,
        name: &str,
        b: &mut FileGraphBuilder,
    ) {
        let names = annotation_names(self.parsed, function);
        let binds = names.iter().any(|a| a == "Binds");
        if !binds && !names.iter().any(|a| a == "Provides") {
            return;
        }
        let Some(provided) = self
            .return_type(function)
            .as_deref()
            .and_then(injected_type)
        else {
            return;
        };
        let target = match (binds, self.param_types(function).into_iter().next()) {
            (true, Some((_, implementation))) => format!("@Binds({provided} <- {implementation})"),
            (true, None) => format!("@Binds({provided})"),
            (false, _) => format!("@Provides({provided})"),
        };
        self.di_edge("provide", target, line(function), container, name, b);
    }

    /// `navController.navigate("detail/$id")` / `navigate(Detail(id))`: ha a célhoz van a fájlban `composable(..)`
    /// útvonal, `nav` él a képernyőre (az argumentum az útvonal).
    pub(super) fn navigate_edge(&self, call: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let Some(target) = navigate_target(self.parsed, call) else {
            return;
        };
        let Some((route, screen)) = match_route(&self.symbols.routes, &target) else {
            return;
        };
        b.call(Call {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: line(call),
            end_line: None,
            kind: "nav".to_string(),
            target: screen.clone(),
            resolved: None,
            on: None,
            category: "component_use".to_string(),
            args: Some(vec![route.clone()]),
            handler: None,
            from: None,
        });
    }
}
