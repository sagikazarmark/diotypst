use crate::observe::{file_id_package, file_id_path};
use crate::workspace::parse_workspace_path;
use crate::{
    DocumentWorkspace, FontSet, PackageBundle, PackageSpec, RenderDate, RenderEnvironment,
    WorkspaceValidationError,
};
use std::path::PathBuf;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Dict, Duration, Value};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Feature, Features, Library, LibraryExt, World};
use typst_kit::files::{FileLoader, FileStore};
use typst_kit::fonts::FontStore;

/// A crate-owned Complete Typst World built from an explicit Typst Project.
pub struct SandboxedWorld {
    files: FileStore<ExplicitFileLoader>,
    library: LazyHash<Library>,
    fonts: FontStore,
    main: FileId,
    render_date: RenderDate,
}

impl SandboxedWorld {
    /// Create a Project World with default Typst library features for paged rendering.
    pub fn new(
        workspace: DocumentWorkspace,
        environment: RenderEnvironment,
    ) -> Result<Self, WorkspaceValidationError> {
        Self::builder(workspace, environment).build()
    }

    /// Create a Project World with Typst's HTML feature enabled.
    pub fn for_html(
        workspace: DocumentWorkspace,
        environment: RenderEnvironment,
    ) -> Result<Self, WorkspaceValidationError> {
        Self::builder(workspace, environment).html().build()
    }

    /// Start building a Project World.
    pub fn builder(
        workspace: DocumentWorkspace,
        environment: RenderEnvironment,
    ) -> SandboxedWorldBuilder {
        SandboxedWorldBuilder {
            workspace,
            environment,
            typst_features: Features::default(),
            #[cfg(feature = "lazy-packages")]
            lazy_package_source: None,
        }
    }

    /// Create a Project World with explicit Typst library features.
    pub fn with_features(
        workspace: DocumentWorkspace,
        environment: RenderEnvironment,
        typst_features: Features,
    ) -> Result<Self, WorkspaceValidationError> {
        Self::builder(workspace, environment)
            .features(typst_features)
            .build()
    }
}

/// Builder for a crate-owned Project World.
pub struct SandboxedWorldBuilder {
    workspace: DocumentWorkspace,
    environment: RenderEnvironment,
    typst_features: Features,
    #[cfg(feature = "lazy-packages")]
    lazy_package_source: Option<std::sync::Arc<dyn crate::SyncPackageSource>>,
}

impl SandboxedWorldBuilder {
    /// Replace the Typst library features used by this world.
    pub fn features(mut self, typst_features: Features) -> Self {
        self.typst_features = typst_features;
        self
    }

    /// Enable Typst's HTML feature for this world.
    pub fn html(self) -> Self {
        self.features([Feature::Html].into_iter().collect())
    }

    /// Resolve packages missing from the Render Environment synchronously during rendering.
    ///
    /// This is the explicit opt-in exception to the closed-world rendering default; see
    /// ADR 0008. Resolution goes through the given synchronous Package Source on the first
    /// file request into a missing package; wrap the source in
    /// [`GatedPackages`](crate::GatedPackages) to apply a Package Policy. Failures are cached
    /// per spec for the lifetime of the world.
    #[cfg(feature = "lazy-packages")]
    pub fn lazy_package_source(
        mut self,
        source: std::sync::Arc<dyn crate::SyncPackageSource>,
    ) -> Self {
        self.lazy_package_source = Some(source);
        self
    }

    /// Build and validate the Project World.
    pub fn build(self) -> Result<SandboxedWorld, WorkspaceValidationError> {
        self.workspace.validate()?;

        // Lazy typst-kit font store: face metadata now, full fonts on first use.
        let fonts = self.environment.font_set().font_store();
        let main = file_id_for_workspace_path(self.workspace.root_path());
        let render_date = self.environment.render_date();
        let library = LazyHash::new(
            Library::builder()
                .with_inputs(self.environment.inputs().clone())
                .with_features(self.typst_features)
                .build(),
        );

        let loader = ExplicitFileLoader::new(self.workspace, self.environment);
        #[cfg(feature = "lazy-packages")]
        let loader = match self.lazy_package_source {
            Some(source) => loader.with_lazy_package_source(source),
            None => loader,
        };
        let files = FileStore::new(loader);

        Ok(SandboxedWorld {
            files,
            library,
            fonts,
            main,
            render_date,
        })
    }
}

impl World for SandboxedWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        Some(self.render_date.to_datetime())
    }
}

struct ExplicitFileLoader {
    workspace: DocumentWorkspace,
    environment: RenderEnvironment,
    #[cfg(feature = "lazy-packages")]
    lazy: Option<LazyPackageResolver>,
}

impl ExplicitFileLoader {
    fn new(workspace: DocumentWorkspace, environment: RenderEnvironment) -> Self {
        Self {
            workspace,
            environment,
            #[cfg(feature = "lazy-packages")]
            lazy: None,
        }
    }

    #[cfg(feature = "lazy-packages")]
    fn with_lazy_package_source(
        mut self,
        source: std::sync::Arc<dyn crate::SyncPackageSource>,
    ) -> Self {
        self.lazy = Some(LazyPackageResolver::new(source));
        self
    }

    fn package_file(&self, package: &PackageSpec, path: &str) -> FileResult<Bytes> {
        if let Some(bundle) = self.environment.package_bundle(package) {
            return bundle_file_bytes(bundle, package, path);
        }

        #[cfg(feature = "lazy-packages")]
        if let Some(lazy) = &self.lazy {
            return lazy.package_file(package, path);
        }

        Err(package_file_not_found(package, path))
    }
}

/// Resolves packages synchronously mid-render through an explicit Package Source.
///
/// Resolved bundles and per-spec failures are cached for the lifetime of the world, so each
/// missing package is resolved at most once.
#[cfg(feature = "lazy-packages")]
struct LazyPackageResolver {
    source: std::sync::Arc<dyn crate::SyncPackageSource>,
    resolved: std::sync::Mutex<crate::PackageBundleSet>,
    failed: std::sync::Mutex<Vec<(PackageSpec, crate::PackageResolveError)>>,
}

#[cfg(feature = "lazy-packages")]
impl LazyPackageResolver {
    fn new(source: std::sync::Arc<dyn crate::SyncPackageSource>) -> Self {
        Self {
            source,
            resolved: std::sync::Mutex::new(crate::PackageBundleSet::new()),
            failed: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn package_file(&self, spec: &PackageSpec, path: &str) -> FileResult<Bytes> {
        {
            let resolved = self.resolved.lock().expect("lazy package lock");
            if let Some(bundle) = resolved.get(spec) {
                return bundle_file_bytes(bundle, spec, path);
            }
        }

        {
            let failed = self.failed.lock().expect("lazy package lock");
            if let Some((_, error)) = failed.iter().find(|(failed_spec, _)| failed_spec == spec) {
                return Err(package_resolve_file_error(spec, error));
            }
        }

        match self.source.resolve_sync(spec) {
            Ok(bundle) => {
                let bytes = bundle_file_bytes(&bundle, spec, path);
                self.resolved
                    .lock()
                    .expect("lazy package lock")
                    .upsert(bundle);
                bytes
            }
            Err(error) => {
                let file_error = package_resolve_file_error(spec, &error);
                self.failed
                    .lock()
                    .expect("lazy package lock")
                    .push((spec.clone(), error));
                Err(file_error)
            }
        }
    }
}

#[cfg(feature = "lazy-packages")]
fn package_resolve_file_error(
    package: &PackageSpec,
    error: &crate::PackageResolveError,
) -> FileError {
    use crate::PackageResolveError;
    use typst::diag::PackageError;

    let package_error = match error {
        PackageResolveError::NotFound { .. } => PackageError::NotFound(package.clone()),
        PackageResolveError::Denied { .. } => PackageError::Other(Some(
            format!("package {package} denied by package policy").into(),
        )),
        PackageResolveError::Retrieval { message, .. } => {
            PackageError::NetworkFailed(Some(message.clone().into()))
        }
        PackageResolveError::Malformed { message, .. } => {
            PackageError::MalformedArchive(Some(message.clone().into()))
        }
        other => PackageError::Other(Some(format!("{other:?}").into())),
    };

    FileError::Package(package_error)
}

fn bundle_file_bytes(
    bundle: &PackageBundle,
    package: &PackageSpec,
    path: &str,
) -> FileResult<Bytes> {
    bundle
        .file_bytes(path)
        .map(|bytes| Bytes::new(bytes.to_vec()))
        .ok_or_else(|| package_file_not_found(package, path))
}

impl FileLoader for ExplicitFileLoader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        let path = file_id_path(id);

        if let Some(package) = file_id_package(id) {
            return self.package_file(package, &path);
        }

        self.workspace
            .file_bytes(&path)
            .map(|bytes| Bytes::new(bytes.to_vec()))
            .ok_or_else(|| FileError::NotFound(PathBuf::from(path)))
    }
}

/// A layered Complete Typst World that overrides explicit resources before delegating to a base world.
pub struct WorldOverlay<W> {
    base: W,
    files: Vec<OverlayFile>,
    package_bundles: crate::PackageBundleSet,
    font_set: OverlayFontSet,
    main: Option<FileId>,
    render_date: Option<RenderDate>,
    library: Option<LazyHash<Library>>,
    inputs: Option<Dict>,
    features: Option<Features>,
}

impl<W> WorldOverlay<W> {
    /// Create an empty World Overlay over a base Complete Typst World.
    pub fn new(base: W) -> Self {
        Self {
            base,
            files: Vec::new(),
            package_bundles: crate::PackageBundleSet::new(),
            font_set: OverlayFontSet::Inherit,
            main: None,
            render_date: None,
            library: None,
            inputs: None,
            features: None,
        }
    }

    /// Replace the Typst values visible through `sys.inputs` for this overlay.
    ///
    /// The overlay rebuilds its Typst library; unless also overridden, library features are
    /// inherited from the base world.
    pub fn inputs(mut self, inputs: Dict) -> Self
    where
        W: World,
    {
        self.inputs = Some(inputs);
        self.rebuild_library();

        self
    }

    /// Replace the Typst library features for this overlay.
    ///
    /// The overlay rebuilds its Typst library; unless also overridden, `sys.inputs` values
    /// are inherited from the base world.
    pub fn features(mut self, features: Features) -> Self
    where
        W: World,
    {
        self.features = Some(features);
        self.rebuild_library();

        self
    }

    fn rebuild_library(&mut self)
    where
        W: World,
    {
        let base_library = self.base.library();
        let inputs = self
            .inputs
            .clone()
            .unwrap_or_else(|| library_inputs(base_library));
        let features = self
            .features
            .clone()
            .unwrap_or_else(|| base_library.features.clone());

        self.library = Some(LazyHash::new(
            Library::builder()
                .with_inputs(inputs)
                .with_features(features)
                .build(),
        ));
    }

    /// Render through this overlay using a different main Typst entrypoint.
    pub fn main(mut self, path: impl Into<String>) -> Result<Self, WorkspaceValidationError> {
        let path = parse_workspace_path(&path.into())?;
        self.main = Some(file_id_for_workspace_path(&path));

        Ok(self)
    }

    /// Override Typst date-sensitive rendering for this overlay.
    pub fn render_date(mut self, render_date: RenderDate) -> Self {
        self.render_date = Some(render_date);

        self
    }

    /// Replace the base world's Font Set for rendering through this overlay.
    pub fn replace_font_set(mut self, font_set: FontSet) -> Self {
        self.font_set = OverlayFontSet::Replace {
            store: font_set.font_store(),
        };

        self
    }

    /// Extend the base world's Font Set for rendering through this overlay.
    pub fn extend_font_set(mut self, font_set: FontSet) -> Self
    where
        W: World,
    {
        let store = font_set.font_store();
        // The combined book: overlay faces first, then the base world's faces, matching
        // the index mapping in `World::font`.
        let mut book = FontBook::new();
        let mut overlay_count = 0;
        while let Some(info) = store.book().info(overlay_count) {
            book.push(info.clone());
            overlay_count += 1;
        }
        let mut base_font_index = 0;
        while let Some(info) = self.base.book().info(base_font_index) {
            book.push(info.clone());
            base_font_index += 1;
        }

        self.font_set = OverlayFontSet::Extend {
            book: LazyHash::new(book),
            store,
            overlay_count,
        };

        self
    }

    /// Add or replace an exact Package Bundle in this overlay.
    pub fn package_bundle(mut self, bundle: PackageBundle) -> Self {
        self.package_bundles.upsert(bundle);
        self
    }

    /// Add or replace a UTF-8 Typst source file in this overlay.
    pub fn source_file(
        self,
        path: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, WorkspaceValidationError> {
        self.file(path, source.into().into_bytes())
    }

    /// Add or replace a binary file in this overlay.
    pub fn file(
        mut self,
        path: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<Self, WorkspaceValidationError> {
        let path = parse_workspace_path(&path.into())?;
        let bytes = bytes.into();

        if let Some(file) = self.files.iter_mut().find(|file| file.path == path) {
            file.bytes = bytes;
        } else {
            self.files.push(OverlayFile { path, bytes });
        }

        Ok(self)
    }

    fn file_bytes(&self, path: impl AsRef<str>) -> Option<&[u8]> {
        let path = VirtualPath::new(path.as_ref()).ok()?;

        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.bytes.as_slice())
    }

    fn package_file(&self, package: &PackageSpec, path: &str) -> Option<FileResult<Bytes>> {
        let bundle = self.package_bundles.get(package)?;

        Some(
            bundle
                .file_bytes(path)
                .map(|bytes| Bytes::new(bytes.to_vec()))
                .ok_or_else(|| package_file_not_found(package, path)),
        )
    }
}

struct OverlayFile {
    path: VirtualPath,
    bytes: Vec<u8>,
}

enum OverlayFontSet {
    Inherit,
    Replace {
        store: FontStore,
    },
    Extend {
        book: LazyHash<FontBook>,
        store: FontStore,
        overlay_count: usize,
    },
}

impl<W> World for WorldOverlay<W>
where
    W: World,
{
    fn library(&self) -> &LazyHash<Library> {
        self.library.as_ref().unwrap_or_else(|| self.base.library())
    }

    fn book(&self) -> &LazyHash<FontBook> {
        match &self.font_set {
            OverlayFontSet::Inherit => self.base.book(),
            OverlayFontSet::Replace { store } => store.book(),
            OverlayFontSet::Extend { book, .. } => book,
        }
    }

    fn main(&self) -> FileId {
        self.main.unwrap_or_else(|| self.base.main())
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if let Some(package) = file_id_package(id) {
            let path = file_id_path(id);
            if let Some(bytes) = self.package_file(package, &path) {
                return source_from_bytes(id, bytes?);
            }

            return self.base.source(id);
        }

        if file_id_package(id).is_none() {
            let path = file_id_path(id);
            if let Some(bytes) = self.file_bytes(&path) {
                return source_from_bytes(id, Bytes::new(bytes.to_vec()));
            }
        }

        self.base.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let path = file_id_path(id);

        if let Some(package) = file_id_package(id) {
            if let Some(bytes) = self.package_file(package, &path) {
                return bytes;
            }

            return self.base.file(id);
        }

        if file_id_package(id).is_none()
            && let Some(bytes) = self.file_bytes(&path)
        {
            return Ok(Bytes::new(bytes.to_vec()));
        }

        self.base.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        match &self.font_set {
            OverlayFontSet::Inherit => self.base.font(index),
            OverlayFontSet::Replace { store } => store.font(index),
            OverlayFontSet::Extend {
                store,
                overlay_count,
                ..
            } => {
                if index < *overlay_count {
                    store.font(index)
                } else {
                    self.base.font(index - overlay_count)
                }
            }
        }
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.render_date
            .map(RenderDate::to_datetime)
            .or_else(|| self.base.today(offset))
    }
}

fn package_file_not_found(package: &PackageSpec, path: &str) -> FileError {
    FileError::NotFound(PathBuf::from(format!("{package}/{path}")))
}

/// Extract the `sys.inputs` Dict from a Typst library.
fn library_inputs(library: &Library) -> Dict {
    let Some(sys) = library.global.scope().get("sys") else {
        return Dict::new();
    };
    let Value::Module(sys) = sys.read() else {
        return Dict::new();
    };
    let Some(inputs) = sys.scope().get("inputs") else {
        return Dict::new();
    };
    let Value::Dict(inputs) = inputs.read() else {
        return Dict::new();
    };

    inputs.clone()
}

fn file_id_for_workspace_path(path: &VirtualPath) -> FileId {
    RootedPath::new(VirtualRoot::Project, path.clone()).intern()
}

fn source_from_bytes(id: FileId, bytes: Bytes) -> FileResult<Source> {
    let text = bytes.as_str().map_err(|_| FileError::InvalidUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).to_owned();

    Ok(Source::new(id, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::foundations::IntoValue;

    fn base_world(inputs: &[(&str, &str)]) -> SandboxedWorld {
        let mut builder = RenderEnvironment::builder().font_set(FontSet::empty());
        for (key, value) in inputs {
            builder = builder.input(*key, *value);
        }

        SandboxedWorld::new(
            DocumentWorkspace::from_source("Hello"),
            builder.build().expect("test environment should build"),
        )
        .expect("test world should build")
    }

    #[test]
    fn overlay_overrides_sys_inputs() {
        let mut inputs = Dict::new();
        inputs.insert("name".into(), "overlay".into_value());

        let overlay = WorldOverlay::new(base_world(&[("name", "base")])).inputs(inputs);

        let extracted = library_inputs(overlay.library());
        assert_eq!(extracted.get("name").ok(), Some(&"overlay".into_value()));
    }

    #[test]
    fn overlay_inputs_inherit_base_features() {
        let world = SandboxedWorld::builder(
            DocumentWorkspace::from_source("Hello"),
            RenderEnvironment::builder()
                .font_set(FontSet::empty())
                .build()
                .expect("test environment should build"),
        )
        .html()
        .build()
        .expect("test world should build");

        let overlay = WorldOverlay::new(world).inputs(Dict::new());

        assert!(overlay.library().features.is_enabled(Feature::Html));
    }

    #[test]
    fn overlay_features_inherit_base_inputs() {
        let overlay = WorldOverlay::new(base_world(&[("name", "base")]))
            .features([Feature::Html].into_iter().collect());

        assert!(overlay.library().features.is_enabled(Feature::Html));
        assert_eq!(
            library_inputs(overlay.library()).get("name").ok(),
            Some(&"base".into_value())
        );
    }
}

#[cfg(all(test, feature = "lazy-packages"))]
mod lazy_tests {
    use super::*;
    use crate::{MemoryPackages, PackageResolveError, SyncPackageSource};
    use std::sync::{Arc, Mutex};

    fn package_file_id(spec: &str, path: &str) -> FileId {
        let spec: PackageSpec = spec.parse().expect("test spec should parse");

        RootedPath::new(
            VirtualRoot::Package(spec),
            VirtualPath::new(path).expect("test path should be a valid VirtualPath"),
        )
        .intern()
    }

    /// A synchronous source counting resolutions to observe lazy caching.
    struct CountingSource {
        inner: MemoryPackages,
        calls: Mutex<usize>,
    }

    impl SyncPackageSource for CountingSource {
        fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
            *self.calls.lock().expect("test lock") += 1;
            self.inner.resolve_sync(spec)
        }
    }

    fn lazy_world(source: Arc<CountingSource>) -> SandboxedWorld {
        SandboxedWorld::builder(
            DocumentWorkspace::from_source("Hello"),
            RenderEnvironment::builder()
                .font_set(FontSet::empty())
                .build()
                .expect("test environment should build"),
        )
        .lazy_package_source(source)
        .build()
        .expect("test world should build")
    }

    #[test]
    fn lazy_package_source_resolves_missing_packages_mid_render() {
        let bundle = PackageBundle::builder(
            "@preview/example:0.1.0"
                .parse()
                .expect("test spec should parse"),
        )
        .file("lib.typ", "#let answer = 42")
        .build()
        .expect("test bundle should build");
        let source = Arc::new(CountingSource {
            inner: MemoryPackages::new([bundle]).expect("test source should build"),
            calls: Mutex::new(0),
        });
        let world = lazy_world(Arc::clone(&source));

        let bytes = world
            .file(package_file_id("@preview/example:0.1.0", "lib.typ"))
            .expect("lazy package file should resolve");
        assert_eq!(&bytes[..], b"#let answer = 42");

        // A second file from the same package reuses the resolved bundle.
        world
            .file(package_file_id("@preview/example:0.1.0", "missing.typ"))
            .expect_err("missing file in resolved package should fail");
        assert_eq!(*source.calls.lock().expect("test lock"), 1);
    }

    #[test]
    fn lazy_package_source_caches_failures_per_spec() {
        let source = Arc::new(CountingSource {
            inner: MemoryPackages::new([]).expect("test source should build"),
            calls: Mutex::new(0),
        });
        let world = lazy_world(Arc::clone(&source));

        let error = world
            .file(package_file_id("@preview/missing:0.1.0", "lib.typ"))
            .expect_err("missing package should fail");
        assert!(matches!(error, FileError::Package(_)));

        world
            .file(package_file_id("@preview/missing:0.1.0", "other.typ"))
            .expect_err("missing package should keep failing");
        assert_eq!(*source.calls.lock().expect("test lock"), 1);
    }
}
