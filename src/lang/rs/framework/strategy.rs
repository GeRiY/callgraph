//! A Rust framework-specifikus tudás felülete.

/// Egy framework: a kimenet `framework` neve, a `Cargo.toml` csomagneve és a hozzá tartozó crate-ek
/// (a hívás-kategorizálás `framework_use`-nak veszi őket).
pub(crate) trait FrameworkStrategy: Sync {
    /// A kimenet `framework` mezője (`"axum"`, `"actix-web"`…).
    fn name(&self) -> &'static str;

    /// A felismeréshez használt `Cargo.toml` függőségnév.
    fn package(&self) -> &'static str;

    /// A framework crate-jei a kódban (aláhúzással: `actix_web`, `tower_http`).
    fn crates(&self) -> &'static [&'static str];

    /// A crate (egy út első szegmense) ehhez a frameworkhöz tartozik-e.
    fn owns_crate(&self, root: &str) -> bool {
        self.crates().contains(&root)
    }
}
