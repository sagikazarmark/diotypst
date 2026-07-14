use crate::package::PackageBundleSet;
use crate::{PackageBundle, PackagePolicy, PackageSpec};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// The official Typst Universe package registry URL.
pub const UNIVERSE_REGISTRY_URL: &str = "https://packages.typst.org";

/// Return the registry archive URL for an exact Package Spec.
///
/// This follows the Typst Universe layout: `{registry}/{namespace}/{name}-{version}.tar.gz`.
/// The official registry only serves the `preview` namespace; callers decide which namespaces
/// a registry actually covers.
pub fn package_archive_url(registry_url: &str, spec: &PackageSpec) -> String {
    format!(
        "{}/{}/{}-{}.tar.gz",
        registry_url.trim_end_matches('/'),
        spec.namespace,
        spec.name,
        spec.version
    )
}

/// Marker for `Send + Sync` bounds that only apply outside wasm targets.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> MaybeSendSync for T {}

/// Marker for `Send + Sync` bounds that only apply outside wasm targets.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSendSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSendSync for T {}

/// The future returned by [`PackageSource::resolve`].
///
/// The future is `Send` on non-wasm targets; on wasm it may be non-`Send` so browser
/// fetch-backed sources can implement [`PackageSource`] directly.
#[cfg(not(target_arch = "wasm32"))]
pub type PackageResolveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PackageBundle, PackageResolveError>> + Send + 'a>>;

/// The future returned by [`PackageSource::resolve`].
///
/// The future is `Send` on non-wasm targets; on wasm it may be non-`Send` so browser
/// fetch-backed sources can implement [`PackageSource`] directly.
#[cfg(target_arch = "wasm32")]
pub type PackageResolveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PackageBundle, PackageResolveError>> + 'a>>;

/// An explicit place a Typst Project may use during World Preparation to resolve packages.
///
/// Resolution is asynchronous so sources may perform network or storage reads during World
/// Preparation; rendering itself never calls a Package Source.
pub trait PackageSource: MaybeSendSync {
    /// Resolve an exact Package Spec into a Package Bundle.
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a>;
}

impl<S: PackageSource + ?Sized> PackageSource for &S {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        (**self).resolve(spec)
    }
}

impl<S: PackageSource + ?Sized> PackageSource for Box<S> {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        (**self).resolve(spec)
    }
}

impl<S: PackageSource + ?Sized> PackageSource for Arc<S> {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        (**self).resolve(spec)
    }
}

/// A Package Source that can resolve synchronously.
///
/// Synchronous sources back the opt-in lazy package resolution seam (see ADR 0008) in
/// addition to World Preparation. They are `Send + Sync` on every target because a lazily
/// resolving world stores its source and `typst::World` itself requires `Send + Sync`.
pub trait SyncPackageSource: Send + Sync {
    /// Resolve an exact Package Spec into a Package Bundle without awaiting.
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError>;
}

impl<S: SyncPackageSource + ?Sized> SyncPackageSource for &S {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        (**self).resolve_sync(spec)
    }
}

impl<S: SyncPackageSource + ?Sized> SyncPackageSource for Box<S> {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        (**self).resolve_sync(spec)
    }
}

impl<S: SyncPackageSource + ?Sized> SyncPackageSource for Arc<S> {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        (**self).resolve_sync(spec)
    }
}

/// Adapts any synchronous Package Source to the asynchronous [`PackageSource`] trait.
pub struct SyncAdapter<S>(pub S);

impl<S: SyncPackageSource> PackageSource for SyncAdapter<S> {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        Box::pin(std::future::ready(self.0.resolve_sync(spec)))
    }
}

/// A Package Source resolution failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageResolveError {
    /// The requested exact package was not available from the source.
    NotFound { spec: PackageSpec },

    /// The package exists, but not in the requested version.
    VersionNotFound {
        /// The requested exact package spec.
        spec: PackageSpec,
        /// The latest version the source knows about.
        latest: typst_syntax::package::PackageVersion,
    },

    /// The requested exact package was denied by a Package Policy.
    Denied { spec: PackageSpec },

    /// The retrieved package data could not be read as a Package Bundle.
    Malformed { spec: PackageSpec, message: String },

    /// The source failed to retrieve the package, such as a network or storage failure.
    Retrieval { spec: PackageSpec, message: String },

    /// Every source in a chain failed with a non-`NotFound` error.
    Exhausted {
        spec: PackageSpec,
        errors: Vec<PackageResolveError>,
    },
}

/// An explicit in-memory source of resolved Package Bundles.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryPackages {
    bundles: PackageBundleSet,
}

impl MemoryPackages {
    /// Create an in-memory Package Source from explicit Package Bundles.
    pub fn new(
        bundles: impl IntoIterator<Item = PackageBundle>,
    ) -> Result<Self, MemoryPackagesError> {
        let bundles = PackageBundleSet::from_bundles(bundles).map_err(|duplicate| {
            MemoryPackagesError::DuplicatePackage {
                spec: duplicate.spec,
            }
        })?;

        Ok(Self { bundles })
    }

    /// Return the Package Bundles held by this source.
    pub fn bundles(&self) -> &[PackageBundle] {
        self.bundles.bundles()
    }
}

impl SyncPackageSource for MemoryPackages {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        self.bundles
            .get(spec)
            .cloned()
            .ok_or_else(|| PackageResolveError::NotFound { spec: spec.clone() })
    }
}

impl PackageSource for MemoryPackages {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        Box::pin(std::future::ready(self.resolve_sync(spec)))
    }
}

/// An in-memory Package Source construction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryPackagesError {
    /// More than one Package Bundle has the same exact package spec.
    DuplicatePackage { spec: PackageSpec },
}

/// An ordered chain of Package Sources tried until one resolves.
///
/// `NotFound` falls through to the next source silently; other errors are recorded and the
/// chain continues. When no source resolves, the chain reports `NotFound` if every source
/// reported `NotFound`, and `Exhausted` with the recorded errors otherwise.
#[derive(Default)]
pub struct PackageSourceChain {
    sources: Vec<Box<dyn PackageSource>>,
}

impl PackageSourceChain {
    /// Create an empty source chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a Package Source to the end of this chain.
    pub fn with(mut self, source: impl PackageSource + 'static) -> Self {
        self.sources.push(Box::new(source));
        self
    }
}

impl PackageSource for PackageSourceChain {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        Box::pin(async move {
            let mut errors = Vec::new();

            for source in &self.sources {
                match source.resolve(spec).await {
                    Ok(bundle) => return Ok(bundle),
                    Err(PackageResolveError::NotFound { .. }) => {}
                    Err(error) => errors.push(error),
                }
            }

            if errors.is_empty() {
                Err(PackageResolveError::NotFound { spec: spec.clone() })
            } else {
                Err(PackageResolveError::Exhausted {
                    spec: spec.clone(),
                    errors,
                })
            }
        })
    }
}

/// An ordered chain of synchronous Package Sources tried until one resolves.
///
/// Chain semantics match [`PackageSourceChain`].
#[derive(Default)]
pub struct SyncPackageSourceChain {
    sources: Vec<Box<dyn SyncPackageSource>>,
}

impl SyncPackageSourceChain {
    /// Create an empty source chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a synchronous Package Source to the end of this chain.
    pub fn with(mut self, source: impl SyncPackageSource + 'static) -> Self {
        self.sources.push(Box::new(source));
        self
    }
}

impl SyncPackageSource for SyncPackageSourceChain {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        let mut errors = Vec::new();

        for source in &self.sources {
            match source.resolve_sync(spec) {
                Ok(bundle) => return Ok(bundle),
                Err(PackageResolveError::NotFound { .. }) => {}
                Err(error) => errors.push(error),
            }
        }

        if errors.is_empty() {
            Err(PackageResolveError::NotFound { spec: spec.clone() })
        } else {
            Err(PackageResolveError::Exhausted {
                spec: spec.clone(),
                errors,
            })
        }
    }
}

impl PackageSource for SyncPackageSourceChain {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        Box::pin(std::future::ready(self.resolve_sync(spec)))
    }
}

/// A Package Source gated by an explicit Package Policy.
///
/// Specs the policy denies resolve to [`PackageResolveError::Denied`] without consulting the
/// inner source.
pub struct GatedPackages<S> {
    inner: S,
    policy: PackagePolicy,
}

impl<S> GatedPackages<S> {
    /// Gate a Package Source behind an explicit Package Policy.
    pub fn new(inner: S, policy: PackagePolicy) -> Self {
        Self { inner, policy }
    }

    /// Return the Package Policy applied by this source.
    pub fn policy(&self) -> &PackagePolicy {
        &self.policy
    }
}

impl<S: PackageSource> PackageSource for GatedPackages<S> {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        if !self.policy.permits(spec) {
            return Box::pin(std::future::ready(Err(PackageResolveError::Denied {
                spec: spec.clone(),
            })));
        }

        self.inner.resolve(spec)
    }
}

impl<S: SyncPackageSource> SyncPackageSource for GatedPackages<S> {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        if !self.policy.permits(spec) {
            return Err(PackageResolveError::Denied { spec: spec.clone() });
        }

        self.inner.resolve_sync(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(spec: &str) -> PackageSpec {
        spec.parse().expect("test spec should parse")
    }

    fn bundle(spec_text: &str) -> PackageBundle {
        PackageBundle::builder(spec(spec_text))
            .file("lib.typ", "#let answer = 42")
            .build()
            .expect("test bundle should build")
    }

    /// A source that fails every resolution with a retrieval error.
    struct FailingPackages;

    impl SyncPackageSource for FailingPackages {
        fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
            Err(PackageResolveError::Retrieval {
                spec: spec.clone(),
                message: "boom".to_owned(),
            })
        }
    }

    impl PackageSource for FailingPackages {
        fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
            Box::pin(std::future::ready(self.resolve_sync(spec)))
        }
    }

    #[test]
    fn memory_packages_resolve_by_exact_spec() {
        let source = MemoryPackages::new([bundle("@preview/cetz:0.4.2")]).unwrap();

        assert!(source.resolve_sync(&spec("@preview/cetz:0.4.2")).is_ok());
        assert_eq!(
            source.resolve_sync(&spec("@preview/cetz:0.3.0")),
            Err(PackageResolveError::NotFound {
                spec: spec("@preview/cetz:0.3.0")
            })
        );
    }

    #[test]
    fn memory_packages_reject_duplicate_specs() {
        let result =
            MemoryPackages::new([bundle("@preview/cetz:0.4.2"), bundle("@preview/cetz:0.4.2")]);

        assert_eq!(
            result,
            Err(MemoryPackagesError::DuplicatePackage {
                spec: spec("@preview/cetz:0.4.2")
            })
        );
    }

    #[test]
    fn chain_falls_through_not_found_and_aggregates_errors() {
        let chain = SyncPackageSourceChain::new()
            .with(MemoryPackages::new([bundle("@preview/oxifmt:1.0.0")]).unwrap())
            .with(MemoryPackages::new([bundle("@preview/cetz:0.4.2")]).unwrap());

        assert!(chain.resolve_sync(&spec("@preview/cetz:0.4.2")).is_ok());
        assert_eq!(
            chain.resolve_sync(&spec("@preview/missing:1.0.0")),
            Err(PackageResolveError::NotFound {
                spec: spec("@preview/missing:1.0.0")
            })
        );

        let failing_chain = SyncPackageSourceChain::new()
            .with(FailingPackages)
            .with(MemoryPackages::new([]).unwrap());
        let error = failing_chain
            .resolve_sync(&spec("@preview/missing:1.0.0"))
            .unwrap_err();

        assert!(matches!(
            error,
            PackageResolveError::Exhausted { ref errors, .. } if errors.len() == 1
        ));
    }

    #[test]
    fn gated_packages_deny_before_resolving() {
        let source = GatedPackages::new(
            MemoryPackages::new([bundle("@preview/cetz:0.4.2")]).unwrap(),
            PackagePolicy::deny_all().allow("@preview/oxifmt".parse().unwrap()),
        );

        assert_eq!(
            source.resolve_sync(&spec("@preview/cetz:0.4.2")),
            Err(PackageResolveError::Denied {
                spec: spec("@preview/cetz:0.4.2")
            })
        );
    }

    #[test]
    fn async_chain_resolves_through_boxed_sources() {
        let chain = PackageSourceChain::new()
            .with(SyncAdapter(FailingPackages))
            .with(MemoryPackages::new([bundle("@preview/cetz:0.4.2")]).unwrap());

        let bundle = pollster::block_on(chain.resolve(&spec("@preview/cetz:0.4.2")));
        assert!(bundle.is_ok());

        let error = pollster::block_on(chain.resolve(&spec("@preview/missing:1.0.0")));
        assert!(matches!(
            error,
            Err(PackageResolveError::Exhausted { ref errors, .. }) if errors.len() == 1
        ));
    }

    #[test]
    fn package_archive_url_follows_universe_layout() {
        assert_eq!(
            package_archive_url(UNIVERSE_REGISTRY_URL, &spec("@preview/cetz:0.4.2")),
            "https://packages.typst.org/preview/cetz-0.4.2.tar.gz"
        );
        assert_eq!(
            package_archive_url(
                "https://example.com/registry/",
                &spec("@preview/cetz:0.4.2")
            ),
            "https://example.com/registry/preview/cetz-0.4.2.tar.gz"
        );
    }
}
