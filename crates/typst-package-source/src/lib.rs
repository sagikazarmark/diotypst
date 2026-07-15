//! Explicit, policy-gated Typst package resolution.
//!
//! This crate owns the Package Source seam and everything that satisfies it: in-memory
//! Package Bundles, policy gating, source chains, verbatim Typst Universe `.tar.gz`
//! archives, Typst CLI-style package directories, and registry downloads. Packages
//! resolve ahead of rendering, during World Preparation, never implicitly mid-compile.
//!
//! The base build depends only on `typst-syntax`, so package tooling (registries,
//! proxies, vendoring, CI) compiles without the Typst compiler or render backends,
//! including on wasm. The filesystem and download sources layer in through features.

#[cfg(feature = "archive")]
mod archive;
#[cfg(feature = "fs-packages")]
mod fs;
mod package;
mod paths;
mod policy;
mod proxy;
#[cfg(feature = "download")]
mod registry;
mod source;
#[cfg(feature = "vendor")]
mod vendor;

#[cfg(feature = "archive")]
pub use archive::PackageArchiveError;
pub use package::{
    DuplicatePackageSpec, PackageBundle, PackageBundleBuilder, PackageBundleError, PackageBundleSet,
};
pub use paths::parse_file_path;
pub use policy::{PackagePattern, PackagePatternError, PackagePolicy};
pub use proxy::{
    PACKAGE_ARCHIVE_CACHE_CONTROL, PACKAGE_ARCHIVE_CONTENT_TYPE, PackageProxyError,
    ProxyArchiveRequest,
};
#[cfg(feature = "download")]
pub use registry::{
    Downloader, Progress, ProgressDownloader, ProgressReporter, RegistryPackages,
    UNIVERSE_NAMESPACE, download_package_archive,
};
pub use source::{
    GatedPackages, MaybeSendSync, MemoryPackages, MemoryPackagesError, PackageResolveError,
    PackageResolveFuture, PackageSource, PackageSourceChain, SyncAdapter, SyncPackageSource,
    SyncPackageSourceChain, UNIVERSE_REGISTRY_URL, package_archive_url,
};
#[cfg(feature = "system-downloader")]
pub use typst_kit::downloader::SystemDownloader;
#[cfg(feature = "vendor")]
pub use vendor::{VendorError, vendor_package_archives};

/// Typst CLI-style package directory source, re-exported from typst-kit.
#[cfg(feature = "fs-packages")]
pub use typst_kit::packages::FsPackages;

/// The Typst CLI package resolution chain (data dir, cache dir, Universe download),
/// re-exported from typst-kit.
#[cfg(feature = "fs-packages")]
pub use typst_kit::packages::SystemPackages;

/// The exact Typst package spec type, re-exported from typst.
pub use typst_syntax::package::PackageSpec;

/// The normalized Typst path type, re-exported from typst.
pub use typst_syntax::VirtualPath;
