#![doc = include_str!("../README.md")]

mod artifact;
mod diagnostics;
mod environment;
mod fonts;
mod observe;
#[cfg(feature = "pack")]
mod pack;
mod prepare;
mod render;
mod workspace;
mod world;

pub use artifact::{
    HtmlArtifact, PageImage, PageImageOptions, PageImagesArtifact, PdfArtifact, RenderArtifact,
    RenderFormat,
};
pub use diagnostics::{RenderDiagnostic, RenderSourceIdentity, RenderSourceRange};
pub use environment::{
    RenderDate, RenderEnvironment, RenderEnvironmentBuilder, RenderEnvironmentError,
};
pub use fonts::FontSet;
#[cfg(feature = "pack")]
pub use pack::{
    PROJECT_PACK_EXTENSION, ProjectPack, ProjectPackBuilder, ProjectPackError, ProjectPackMetadata,
};
pub use prepare::{
    PackagePreparation, PrepareEvent, PreparePackagesOptions, prepare_packages,
    prepare_packages_with_progress,
};
pub use render::{
    PackageDependencyObservation, PackageDependencyTarget, RenderError,
    observe_package_dependencies, observe_package_dependencies_world, render_artifact,
    render_artifact_world,
};
#[cfg(feature = "html")]
pub use render::{render_html, render_html_world};
#[cfg(feature = "page-images")]
pub use render::{render_page_images, render_page_images_world};
#[cfg(feature = "pdf")]
pub use render::{render_pdf, render_pdf_world};
pub use workspace::{
    DocumentWorkspace, DocumentWorkspaceBuilder, WorkspaceFile, WorkspaceValidationError,
};
pub use world::{SandboxedWorld, SandboxedWorldBuilder, WorldOverlay};

// Package acquisition lives in typst-package-source (a typst-syntax-tier crate, so
// package tooling compiles without this crate's compiler and render backends); the
// whole surface is re-exported here for one-import ergonomics.
#[cfg(feature = "archive")]
pub use typst_package_source::PackageArchiveError;
#[cfg(feature = "system-downloader")]
pub use typst_package_source::SystemDownloader;
#[cfg(feature = "download")]
pub use typst_package_source::{
    Downloader, Progress, ProgressDownloader, ProgressReporter, RegistryPackages,
    UNIVERSE_NAMESPACE, download_package_archive,
};
pub use typst_package_source::{
    DuplicatePackageSpec, GatedPackages, MaybeSendSync, MemoryPackages, MemoryPackagesError,
    PackageBundle, PackageBundleBuilder, PackageBundleError, PackageBundleSet, PackagePattern,
    PackagePatternError, PackagePolicy, PackageResolveError, PackageResolveFuture, PackageSource,
    PackageSourceChain, SyncAdapter, SyncPackageSource, SyncPackageSourceChain,
    UNIVERSE_REGISTRY_URL, package_archive_url,
};
#[cfg(feature = "fs-packages")]
pub use typst_package_source::{FsPackages, SystemPackages};
#[cfg(feature = "vendor")]
pub use typst_package_source::{VendorError, vendor_package_archives};

/// The exact Typst package spec type, re-exported from typst.
pub use typst::syntax::package::PackageSpec;

/// The normalized Typst path type, re-exported from typst.
pub use typst::syntax::VirtualPath;
