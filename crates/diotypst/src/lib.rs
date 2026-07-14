#![doc = include_str!("../README.md")]

mod component;
mod download;
#[cfg(feature = "dioxus")]
mod import;
mod package_proxy;
mod packages;
mod preparation;
mod provider;
mod render_state;
mod server;
mod session;

#[cfg(feature = "dioxus")]
pub use component::Typst;
#[cfg(target_arch = "wasm32")]
pub use download::{BrowserDownloadError, trigger_browser_download};
pub use download::{
    DownloadError, DownloadFile, DownloadFormat, RenderDownloadError, render_download,
};
#[cfg(feature = "dioxus")]
pub use import::{
    FileImportError, FileImportOptions, ImportedFileKind, ImportedProjectFile,
    import_project_files, is_font_file, partition_imported_fonts, project_path_from_import,
};
#[cfg(feature = "archive")]
pub use libtypst::PackageArchiveError;
#[cfg(feature = "system-downloader")]
pub use libtypst::SystemDownloader;
pub use libtypst::{
    DocumentWorkspace, DocumentWorkspaceBuilder, VirtualPath, WorkspaceFile,
    WorkspaceValidationError,
};
#[cfg(feature = "download")]
pub use libtypst::{
    Downloader, Progress, ProgressDownloader, ProgressReporter, RegistryPackages,
    UNIVERSE_NAMESPACE, download_package_archive,
};
pub use libtypst::{
    FontSet, RenderDate, RenderEnvironment, RenderEnvironmentBuilder, RenderEnvironmentError,
};
#[cfg(feature = "fs-packages")]
pub use libtypst::{FsPackages, SystemPackages};
pub use libtypst::{
    GatedPackages, MaybeSendSync, MemoryPackages, MemoryPackagesError, PackageResolveError,
    PackageResolveFuture, PackageSource, PackageSourceChain, SyncAdapter, SyncPackageSource,
    SyncPackageSourceChain, UNIVERSE_REGISTRY_URL, package_archive_url,
};
pub use libtypst::{
    HtmlArtifact, PageImage, PageImageOptions, PageImagesArtifact, PdfArtifact, RenderArtifact,
    RenderFormat,
};
#[cfg(feature = "pack")]
pub use libtypst::{
    PROJECT_PACK_EXTENSION, ProjectPack, ProjectPackBuilder, ProjectPackError, ProjectPackMetadata,
};
pub use libtypst::{PackageBundle, PackageBundleBuilder, PackageBundleError, PackageSpec};
pub use libtypst::{
    PackageDependencyObservation, PackageDependencyTarget, RenderError,
    observe_package_dependencies, observe_package_dependencies_world, render_artifact,
    render_artifact_world,
};
pub use libtypst::{PackagePattern, PackagePatternError, PackagePolicy};
pub use libtypst::{
    PackagePreparation, PrepareEvent, PreparePackagesOptions, prepare_packages,
    prepare_packages_with_progress,
};
pub use libtypst::{RenderDiagnostic, RenderSourceIdentity, RenderSourceRange};
pub use libtypst::{SandboxedWorld, SandboxedWorldBuilder, WorldOverlay};
#[cfg(feature = "vendor")]
pub use libtypst::{VendorError, vendor_package_archives};
#[cfg(feature = "html")]
pub use libtypst::{render_html, render_html_world};
#[cfg(feature = "page-images")]
pub use libtypst::{render_page_images, render_page_images_world};
#[cfg(feature = "pdf")]
pub use libtypst::{render_pdf, render_pdf_world};
#[cfg(all(feature = "server", feature = "download"))]
pub use package_proxy::DownloaderArchiveFetcher;
pub use package_proxy::{
    PACKAGE_ARCHIVE_CACHE_CONTROL, PACKAGE_ARCHIVE_CONTENT_TYPE, PackageProxyError,
    ProxyArchiveRequest, SERVER_PACKAGE_PROXY_PATH,
};
#[cfg(feature = "server")]
pub use package_proxy::{
    PackageArchiveFetchError, PackageArchiveFetcher, PackageProxyConfig,
    server_package_proxy_router,
};
#[cfg(all(target_arch = "wasm32", feature = "archive"))]
pub use packages::FetchPackageSource;
pub use packages::SERVER_PACKAGE_PROXY_BASE;
pub use preparation::{
    PackagePreparationEntry, PackagePreparationStatus, WorldPreparationPhase, WorldPreparationState,
};
pub use provider::{SharedPackageSource, TypstProviderDefaults};
#[cfg(feature = "dioxus")]
pub use provider::{TypstProvider, use_typst_defaults};
#[cfg(feature = "dioxus")]
pub use render_state::use_typst_render;
pub use render_state::{HeadlessRender, RenderState, RenderStatus};
pub use server::ServerRenderRequest;
#[cfg(feature = "server")]
pub use server::{
    SERVER_RENDER_DOWNLOAD_PATH, server_render_download_handler, server_render_download_response,
    server_render_download_router,
};
#[cfg(feature = "dioxus")]
pub use session::{RenderSession, use_render_session};
pub use session::{RenderSessionOptions, TypstInput, TypstView};

// This integration-style suite assumes every render backend and bundled fonts.
#[cfg(all(
    test,
    feature = "bundled-fonts",
    feature = "html",
    feature = "page-images",
    feature = "pdf"
))]
mod tests;
