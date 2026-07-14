use crate::{
    DocumentWorkspace, FontSet, PackageBundle, PackageBundleError, PackageBundleSet, PackageSpec,
    RenderEnvironment, RenderEnvironmentError, WorkspaceValidationError,
};

/// The conventional file extension for Project Pack archives.
pub const PROJECT_PACK_EXTENSION: &str = typst_pack::FILE_EXTENSION;

/// A portable single-file archive (`.typk`) of a Typst Project.
///
/// A Project Pack carries everything needed to render offline: the Typst
/// Project itself, vendored Package Bundles, external package specs that must
/// still be resolved through a Package Source, and optional embedded font
/// files for the Font Set. The archive format is defined by the independent
/// [`typst-pack`](https://github.com/sagikazarmark/typst-pack) crate; this
/// type converts packs to and from this crate's domain types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPack {
    project: DocumentWorkspace,
    package_bundles: PackageBundleSet,
    external_packages: Vec<PackageSpec>,
    font_files: Vec<Vec<u8>>,
    metadata: Option<ProjectPackMetadata>,
}

impl ProjectPack {
    /// Start building a Project Pack from an already-validated Typst Project.
    pub fn builder(project: DocumentWorkspace) -> ProjectPackBuilder {
        ProjectPackBuilder {
            project,
            package_bundles: Vec::new(),
            external_packages: Vec::new(),
            font_files: Vec::new(),
            metadata: None,
        }
    }

    /// Parse a `.typk` archive into a Project Pack.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, ProjectPackError> {
        let pack = typst_pack::Pack::from_bytes(bytes.as_ref().to_vec()).map_err(|error| {
            ProjectPackError::Archive {
                message: error.to_string(),
            }
        })?;

        let mut project = DocumentWorkspace::builder(pack.entrypoint());
        for (path, data) in pack.files() {
            project = project.file(path, data.as_slice());
        }
        let project = project.build().map_err(ProjectPackError::Project)?;

        let mut package_bundles = PackageBundleSet::new();
        for (spec, files) in pack.packages() {
            let mut bundle = PackageBundle::builder(spec.clone());
            for (path, data) in files {
                bundle = bundle.file(path, data.as_slice());
            }
            let bundle = bundle.build().map_err(|error| ProjectPackError::Package {
                spec: spec.to_string(),
                error,
            })?;
            package_bundles.try_insert(bundle).map_err(|duplicate| {
                ProjectPackError::DuplicatePackage {
                    spec: duplicate.spec,
                }
            })?;
        }

        let external_packages = pack
            .manifest()
            .external_packages()
            .map_err(|error| ProjectPackError::Archive {
                message: error.to_string(),
            })?
            .to_vec();

        // Faces of one font collection share an archive entry; keep each
        // font file once for the Font Set.
        let mut font_paths = std::collections::HashSet::new();
        let mut font_files = Vec::new();
        for font in pack.fonts() {
            if font_paths.insert(font.entry.path.clone()) {
                font_files.push(font.data.to_vec());
            }
        }

        let metadata = pack.manifest().metadata.as_ref().map(|metadata| {
            let mut converted = ProjectPackMetadata::new();
            if let Some(name) = &metadata.name {
                converted = converted.with_name(name);
            }
            if let Some(description) = &metadata.description {
                converted = converted.with_description(description);
            }
            for author in &metadata.authors {
                converted = converted.with_author(author);
            }
            converted
        });

        Ok(Self {
            project,
            package_bundles,
            external_packages,
            font_files,
            metadata,
        })
    }

    /// Serialize this Project Pack into `.typk` archive bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProjectPackError> {
        let archive_error = |error: &dyn std::fmt::Display| ProjectPackError::Archive {
            message: error.to_string(),
        };

        let mut pack = typst_pack::Pack::builder(self.project.root_path().get_without_slash());
        for file in self.project.files() {
            pack = pack
                .file(file.path().get_without_slash(), file.bytes())
                .map_err(|error| archive_error(&error))?;
        }

        for bundle in self.package_bundles.bundles() {
            for (path, data) in bundle.files() {
                pack = pack
                    .package_file(bundle.spec().clone(), path, data)
                    .map_err(|error| archive_error(&error))?;
            }
        }

        for spec in &self.external_packages {
            pack = pack.external_package(spec.clone());
        }

        for data in &self.font_files {
            // Embed every face of a font collection; plain font files have
            // exactly one face at index zero.
            let mut index = 0;
            while typst::text::FontInfo::new(data, index).is_some() {
                pack = pack
                    .font(data.clone(), index)
                    .map_err(|error| archive_error(&error))?;
                index += 1;
            }
            if index == 0 {
                return Err(ProjectPackError::UnrecognizedFont);
            }
        }

        if let Some(metadata) = &self.metadata {
            pack = pack.metadata(typst_pack::Metadata {
                name: metadata.name.clone(),
                description: metadata.description.clone(),
                authors: metadata.authors.clone(),
            });
        }

        let pack = pack.build().map_err(|error| archive_error(&error))?;

        pack.to_bytes().map_err(|error| archive_error(&error))
    }

    /// Return the packed Typst Project.
    pub fn project(&self) -> &DocumentWorkspace {
        &self.project
    }

    /// Return the vendored Package Bundles.
    pub fn package_bundles(&self) -> &[PackageBundle] {
        self.package_bundles.bundles()
    }

    /// Return the observed package dependencies that are not vendored and
    /// must still be resolved through a Package Source.
    pub fn external_packages(&self) -> &[PackageSpec] {
        &self.external_packages
    }

    /// Return the embedded font files.
    pub fn font_files(&self) -> &[Vec<u8>] {
        &self.font_files
    }

    /// Return the optional descriptive metadata.
    pub fn metadata(&self) -> Option<&ProjectPackMetadata> {
        self.metadata.as_ref()
    }

    /// Return the Font Set for rendering this pack: the default fonts for this
    /// build plus any embedded font files.
    pub fn font_set(&self) -> FontSet {
        FontSet::default().with_font_files(self.font_files.clone())
    }

    /// Build a Render Environment from this pack's Package Bundles and fonts.
    ///
    /// External packages are not resolved here; chain
    /// [`RenderEnvironment::to_builder`] with prepared bundles when
    /// [`external_packages`](Self::external_packages) is not empty.
    pub fn render_environment(&self) -> Result<RenderEnvironment, RenderEnvironmentError> {
        RenderEnvironment::builder()
            .package_bundles(self.package_bundles.bundles().iter().cloned())
            .font_set(self.font_set())
            .build()
    }
}

/// Builder for a [`ProjectPack`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPackBuilder {
    project: DocumentWorkspace,
    package_bundles: Vec<PackageBundle>,
    external_packages: Vec<PackageSpec>,
    font_files: Vec<Vec<u8>>,
    metadata: Option<ProjectPackMetadata>,
}

impl ProjectPackBuilder {
    /// Vendor a Package Bundle inside the pack.
    pub fn package_bundle(mut self, bundle: PackageBundle) -> Self {
        self.package_bundles.push(bundle);
        self
    }

    /// Vendor Package Bundles inside the pack.
    pub fn package_bundles(mut self, bundles: impl IntoIterator<Item = PackageBundle>) -> Self {
        self.package_bundles.extend(bundles);
        self
    }

    /// Record an observed package dependency without vendoring its files.
    pub fn external_package(mut self, spec: PackageSpec) -> Self {
        if !self.external_packages.contains(&spec) {
            self.external_packages.push(spec);
        }
        self
    }

    /// Embed a font file; collections contribute every face.
    pub fn font_file(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.font_files.push(bytes.into());
        self
    }

    /// Attach descriptive metadata to the pack.
    pub fn metadata(mut self, metadata: ProjectPackMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Build and validate the Project Pack.
    pub fn build(self) -> Result<ProjectPack, ProjectPackError> {
        self.project.validate().map_err(ProjectPackError::Project)?;

        let package_bundles =
            PackageBundleSet::from_bundles(self.package_bundles).map_err(|duplicate| {
                ProjectPackError::DuplicatePackage {
                    spec: duplicate.spec,
                }
            })?;

        for data in &self.font_files {
            if typst::text::FontInfo::new(data, 0).is_none() {
                return Err(ProjectPackError::UnrecognizedFont);
            }
        }

        Ok(ProjectPack {
            project: self.project,
            package_bundles,
            external_packages: self.external_packages,
            font_files: self.font_files,
            metadata: self.metadata,
        })
    }
}

/// Optional descriptive metadata carried by a Project Pack.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectPackMetadata {
    name: Option<String>,
    description: Option<String>,
    authors: Vec<String>,
}

impl ProjectPackMetadata {
    /// Create empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a display name for the packed project.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set a description for the packed project.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an author of the packed project.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.authors.push(author.into());
        self
    }

    /// Return the display name, if set.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Return the description, if set.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Return the authors.
    pub fn authors(&self) -> &[String] {
        &self.authors
    }
}

/// A Project Pack read, validation, or write failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectPackError {
    /// The bytes could not be read or written as a `.typk` archive.
    Archive { message: String },

    /// The packed files do not form a valid Typst Project.
    Project(WorkspaceValidationError),

    /// A vendored package could not be converted into a Package Bundle.
    Package {
        spec: String,
        error: PackageBundleError,
    },

    /// More than one vendored Package Bundle has the same exact package spec.
    DuplicatePackage { spec: PackageSpec },

    /// An embedded font file could not be parsed as a font.
    UnrecognizedFont,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pack() -> ProjectPack {
        let project = DocumentWorkspace::builder("main.typ")
            .source_file(
                "main.typ",
                "#import \"@demo/badge:0.1.0\": badge\n#include \"chapters/intro.typ\"",
            )
            .source_file("chapters/intro.typ", "= Intro")
            .file("assets/logo.png", b"\x89PNG".to_vec())
            .build()
            .expect("sample project should be valid");
        let bundle = PackageBundle::builder(
            "@demo/badge:0.1.0"
                .parse()
                .expect("sample spec should parse"),
        )
        .file("typst.toml", b"[package]".to_vec())
        .file("lib.typ", b"#let badge(body) = body".to_vec())
        .build()
        .expect("sample bundle should be valid");

        ProjectPack::builder(project)
            .package_bundle(bundle)
            .external_package(
                "@preview/cetz:0.4.2"
                    .parse()
                    .expect("external spec should parse"),
            )
            .metadata(
                ProjectPackMetadata::new()
                    .with_name("Sample")
                    .with_author("Demo"),
            )
            .build()
            .expect("sample pack should build")
    }

    #[test]
    fn project_pack_round_trips_through_typk_bytes() {
        let pack = sample_pack();

        let bytes = pack.to_bytes().expect("pack should serialize");
        let read = ProjectPack::from_bytes(&bytes).expect("pack should parse back");

        assert_eq!(read.project().root_path().get_without_slash(), "main.typ");
        assert_eq!(
            read.project().file_bytes("chapters/intro.typ"),
            Some(b"= Intro".as_slice())
        );
        assert_eq!(read.package_bundles().len(), 1);
        assert_eq!(
            read.package_bundles()[0].file_bytes("lib.typ"),
            Some(b"#let badge(body) = body".as_slice())
        );
        assert_eq!(
            read.external_packages(),
            &["@preview/cetz:0.4.2"
                .parse::<PackageSpec>()
                .expect("external spec should parse")]
        );
        let metadata = read.metadata().expect("metadata should survive");
        assert_eq!(metadata.name(), Some("Sample"));
        assert_eq!(metadata.authors(), ["Demo".to_owned()]);
    }

    #[test]
    fn project_pack_embeds_and_restores_font_files() {
        let font = typst_assets::fonts()
            .next()
            .expect("bundled fonts should not be empty")
            .to_vec();
        let pack = ProjectPack::builder(DocumentWorkspace::from_source("Hello"))
            .font_file(font.clone())
            .build()
            .expect("pack with a font should build");

        let bytes = pack.to_bytes().expect("pack should serialize");
        let read = ProjectPack::from_bytes(&bytes).expect("pack should parse back");

        assert_eq!(read.font_files(), &[font]);
        assert_ne!(read.font_set(), FontSet::bundled());
    }

    #[test]
    fn project_pack_render_environment_installs_vendored_bundles() {
        let environment = sample_pack()
            .render_environment()
            .expect("environment should build");

        let bundle = environment
            .package_bundle(&"@demo/badge:0.1.0".parse().expect("spec should parse"))
            .expect("vendored bundle should be installed");
        assert_eq!(
            bundle.file_bytes("typst.toml"),
            Some(b"[package]".as_slice())
        );
    }

    #[test]
    fn project_pack_rejects_garbage_bytes() {
        let result = ProjectPack::from_bytes(b"not a pack");

        assert!(matches!(result, Err(ProjectPackError::Archive { .. })));
    }

    #[test]
    fn project_pack_builder_rejects_unrecognized_fonts_and_duplicate_packages() {
        let font_result = ProjectPack::builder(DocumentWorkspace::from_source("Hello"))
            .font_file(b"not a font".to_vec())
            .build();
        assert_eq!(font_result, Err(ProjectPackError::UnrecognizedFont));

        let bundle = |spec: &str| {
            PackageBundle::builder(spec.parse().expect("spec should parse"))
                .file("lib.typ", b"".to_vec())
                .build()
                .expect("bundle should build")
        };
        let duplicate_result = ProjectPack::builder(DocumentWorkspace::from_source("Hello"))
            .package_bundle(bundle("@demo/badge:0.1.0"))
            .package_bundle(bundle("@demo/badge:0.1.0"))
            .build();
        assert_eq!(
            duplicate_result,
            Err(ProjectPackError::DuplicatePackage {
                spec: "@demo/badge:0.1.0".parse().expect("spec should parse"),
            })
        );
    }

    #[test]
    fn project_pack_reads_archives_written_by_typst_pack_directly() {
        // Interop guard: a pack assembled with the raw typst-pack builder,
        // not just our own writer, converts into crate domain types.
        let pack = typst_pack::Pack::builder("main.typ")
            .file("main.typ", b"Hello".to_vec())
            .expect("file should be valid")
            .build()
            .expect("raw pack should build")
            .to_bytes()
            .expect("raw pack should serialize");

        let read = ProjectPack::from_bytes(&pack).expect("raw pack should parse");

        assert_eq!(read.project().root_path().get_without_slash(), "main.typ");
        assert_eq!(
            read.project().file_bytes("main.typ"),
            Some(b"Hello".as_slice())
        );
    }
}
