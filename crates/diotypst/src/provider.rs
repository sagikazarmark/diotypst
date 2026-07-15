use crate::{PackageSource, RenderEnvironment};
#[cfg(feature = "dioxus")]
use dioxus::prelude::{
    Element, Props, component, dioxus_core, try_use_context, use_context_provider,
};

/// A Package Source shared through a Typst Provider.
///
/// This wraps a reference-counted [`PackageSource`] so provider defaults stay cheap to clone
/// and comparable; equality is provider identity (pointer equality), not source contents.
#[derive(Clone)]
pub struct SharedPackageSource(std::sync::Arc<dyn PackageSource>);

impl SharedPackageSource {
    /// Share a Package Source.
    pub fn new(source: impl PackageSource + 'static) -> Self {
        Self(std::sync::Arc::new(source))
    }
}

impl PackageSource for SharedPackageSource {
    fn resolve<'a>(
        &'a self,
        spec: &'a typst_project::PackageSpec,
    ) -> typst_project::PackageResolveFuture<'a> {
        self.0.resolve(spec)
    }
}

impl PartialEq for SharedPackageSource {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for SharedPackageSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedPackageSource")
            .finish_non_exhaustive()
    }
}

/// Shared Typst rendering defaults supplied by a Typst Provider.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypstProviderDefaults {
    environment: RenderEnvironment,
    package_source: Option<SharedPackageSource>,
}

impl TypstProviderDefaults {
    /// Create provider defaults with the given Render Environment.
    pub fn new(environment: RenderEnvironment) -> Self {
        Self {
            environment,
            package_source: None,
        }
    }

    /// Share a Package Source for World Preparation with provider consumers.
    ///
    /// Wrap the source in [`GatedPackages`](typst_project::GatedPackages) to apply a Package
    /// Policy before sharing it.
    pub fn with_package_source(mut self, package_source: SharedPackageSource) -> Self {
        self.package_source = Some(package_source);
        self
    }

    /// Add or replace one Typst value visible through `sys.inputs` in the default environment.
    pub fn with_input(
        mut self,
        key: impl Into<String>,
        value: impl typst::foundations::IntoValue,
    ) -> Self {
        self.environment = self
            .environment
            .to_builder()
            .input(key, value)
            .build()
            .expect("adding sys.inputs cannot invalidate a valid render environment");
        self
    }

    /// Return the default Render Environment.
    pub fn environment(&self) -> &RenderEnvironment {
        &self.environment
    }

    /// Return the shared Package Source, if one was provided.
    pub fn package_source(&self) -> Option<&SharedPackageSource> {
        self.package_source.as_ref()
    }
}

/// Return Typst Provider defaults from context, or default rendering values when no provider exists.
#[cfg(feature = "dioxus")]
pub fn use_typst_defaults() -> TypstProviderDefaults {
    try_use_context::<TypstProviderDefaults>().unwrap_or_default()
}

/// Provide shared Typst rendering defaults to Dioxus descendants.
#[cfg(feature = "dioxus")]
#[component]
pub fn TypstProvider(defaults: TypstProviderDefaults, children: Element) -> Element {
    use_context_provider(|| defaults);

    children
}
