use crate::{
    FontSet, PackageBundle, PackageBundleError, PackageBundleSet, PackageSpec, Project,
    ProjectValidationError, RenderDate, RenderEnvironment, RenderEnvironmentError,
};
use typst::foundations::{Dict, IntoValue};

/// The conventional file extension for Project Pack archives.
pub const PROJECT_PACK_EXTENSION: &str = typst_pack::FILE_EXTENSION;

/// A portable single-file archive (`.typk`) of a Typst Project.
///
/// A Project Pack carries everything needed to render offline: the Typst
/// Project itself, vendored Package Bundles, exact external package-tree
/// requirements that must still be fulfilled, and optional embedded font
/// files for the Font Set. The archive format is defined by the independent
/// [`typst-pack`](https://github.com/sagikazarmark/typst-pack) crate; this
/// type converts packs to and from this crate's domain types.
#[derive(Clone, Debug)]
pub struct ProjectPack {
    project: Project,
    package_bundles: PackageBundleSet,
    external_packages: Vec<PackageSpec>,
    external_package_requirements: Vec<ExternalPackageRequirement>,
    external_package_bundles: Vec<PackageBundle>,
    font_catalog: Vec<ProjectPackFontFace>,
    external_font_requirements: Vec<ExternalFontRequirement>,
    font_files: Vec<Vec<u8>>,
    builder_font_faces: Vec<ProjectPackBuilderFontFace>,
    metadata: Option<ProjectPackMetadata>,
    source_bytes: Option<Vec<u8>>,
}

impl PartialEq for ProjectPack {
    fn eq(&self, other: &Self) -> bool {
        self.project == other.project
            && self.package_bundles == other.package_bundles
            && self.external_packages == other.external_packages
            && self.external_package_requirements == other.external_package_requirements
            && self.font_catalog == other.font_catalog
            && self.external_font_requirements == other.external_font_requirements
            && self.font_files == other.font_files
            && self.metadata == other.metadata
    }
}

impl Eq for ProjectPack {}

impl ProjectPack {
    /// Start building a Project Pack from an already-validated Typst Project.
    pub fn builder(project: Project) -> ProjectPackBuilder {
        ProjectPackBuilder {
            project,
            package_bundles: Vec::new(),
            external_package_bundles: Vec::new(),
            font_faces: Vec::new(),
            invalid_font: false,
            metadata: None,
        }
    }

    /// Parse a `.typk` archive into a Project Pack.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Result<Self, ProjectPackError> {
        let source_bytes = bytes.as_ref().to_vec();
        let pack = typst_pack::Pack::from_bytes(source_bytes.clone()).map_err(|error| {
            ProjectPackError::Archive {
                message: error.to_string(),
            }
        })?;

        let mut project = Project::builder(pack.entrypoint());
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

        let external_package_requirements = pack
            .package_requirements()
            .iter()
            .filter(|requirement| !requirement.is_embedded())
            .map(|requirement| ExternalPackageRequirement {
                spec: requirement.spec().clone(),
                tree_digest: requirement.tree_identity().digest(),
                file_count: requirement.file_count(),
                byte_length: requirement.byte_length(),
            })
            .collect::<Vec<_>>();
        let external_packages = external_package_requirements
            .iter()
            .map(|requirement| requirement.spec.clone())
            .collect();

        let external_font_requirements = pack
            .font_requirements()
            .iter()
            .filter(|requirement| !requirement.is_embedded())
            .map(|requirement| ExternalFontRequirement {
                container_digest: requirement.container_identity().digest(),
                container_length: requirement.container_length(),
                face_indices: requirement.face_indices().to_vec(),
            })
            .collect();
        let font_catalog = pack
            .font_catalog()
            .iter()
            .map(|face| ProjectPackFontFace {
                container_digest: face.identity().container().digest(),
                index: face.identity().index(),
                embedded: face.is_embedded(),
            })
            .collect();

        let mut font_containers = std::collections::HashSet::new();
        let mut font_files = Vec::new();
        for font in pack.fonts() {
            let digest = typst_pack::FontContainerIdentity::from_bytes(font.data()).digest();
            if font_containers.insert(digest) {
                font_files.push(font.data().to_vec());
            }
        }

        let metadata = pack.manifest().metadata().map(|metadata| {
            let mut converted = ProjectPackMetadata::new();
            if let Some(name) = metadata.name() {
                converted = converted.with_name(name);
            }
            if let Some(description) = metadata.description() {
                converted = converted.with_description(description);
            }
            for author in metadata.authors() {
                converted = converted.with_author(author);
            }
            converted
        });

        Ok(Self {
            project,
            package_bundles,
            external_packages,
            external_package_requirements,
            external_package_bundles: Vec::new(),
            font_catalog,
            external_font_requirements,
            font_files,
            builder_font_faces: Vec::new(),
            metadata,
            source_bytes: Some(source_bytes),
        })
    }

    /// Serialize this Project Pack into `.typk` archive bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProjectPackError> {
        if let Some(bytes) = &self.source_bytes {
            return Ok(bytes.clone());
        }

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

        for bundle in &self.external_package_bundles {
            for (path, data) in bundle.files() {
                pack = pack
                    .external_package_file(bundle.spec().clone(), path, data)
                    .map_err(|error| archive_error(&error))?;
            }
        }

        for face in &self.builder_font_faces {
            pack = if face.embedded {
                pack.font(face.data.clone(), face.index)
            } else {
                pack.external_font(face.data.clone(), face.index)
            }
            .map_err(|error| archive_error(&error))?;
        }

        if let Some(metadata) = &self.metadata {
            let mut converted = typst_pack::PackMetadata::new();
            if let Some(name) = &metadata.name {
                converted = converted.with_name(name);
            }
            if let Some(description) = &metadata.description {
                converted = converted.with_description(description);
            }
            for author in &metadata.authors {
                converted = converted.with_author(author);
            }
            pack = pack.metadata(converted);
        }

        let pack = pack.build().map_err(|error| archive_error(&error))?;

        pack.to_bytes().map_err(|error| archive_error(&error))
    }

    /// Return the packed Typst Project.
    pub fn project(&self) -> &Project {
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

    /// Return the exact external package-tree requirements carried by the pack.
    pub fn external_package_requirements(&self) -> &[ExternalPackageRequirement] {
        &self.external_package_requirements
    }

    /// Verify that an environment fulfills every exact external package-tree
    /// requirement carried by this pack.
    pub fn verify_external_packages(
        &self,
        environment: &RenderEnvironment,
    ) -> Result<(), ProjectPackError> {
        verify_external_package_requirements(&self.external_package_requirements, |spec| {
            environment.package_bundle(spec)
        })
    }

    /// Return the embedded font files.
    pub fn font_files(&self) -> &[Vec<u8>] {
        &self.font_files
    }

    /// Return the exact external font-container requirements carried by the pack.
    pub fn external_font_requirements(&self) -> &[ExternalFontRequirement] {
        &self.external_font_requirements
    }

    /// Return the optional descriptive metadata.
    pub fn metadata(&self) -> Option<&ProjectPackMetadata> {
        self.metadata.as_ref()
    }

    /// Return the embedded portion of this pack's exact Font Set.
    pub fn font_set(&self) -> FontSet {
        let containers = self.font_containers(std::iter::empty::<&Vec<u8>>());
        FontSet::from_font_faces(
            self.font_catalog
                .iter()
                .filter(|face| face.embedded)
                .map(|face| (containers[&face.container_digest].clone(), face.index)),
        )
    }

    /// Start building a render environment from this pack.
    pub fn environment_builder(&self) -> ProjectPackEnvironmentBuilder<'_> {
        ProjectPackEnvironmentBuilder {
            pack: self,
            package_bundles: Vec::new(),
            font_files: Vec::new(),
            render_date: RenderDate::default(),
            inputs: Dict::new(),
        }
    }

    /// Build a render-ready Render Environment from this pack.
    ///
    /// This succeeds directly for a self-contained pack and reports any exact
    /// external package or font requirement that still needs fulfillment.
    pub fn render_environment(&self) -> Result<RenderEnvironment, ProjectPackError> {
        self.environment_builder().build()
    }

    /// Build a render environment using a base environment as the source for
    /// Render Context and exact external resource fulfillments.
    pub fn render_environment_from(
        &self,
        base: &RenderEnvironment,
    ) -> Result<RenderEnvironment, ProjectPackError> {
        self.environment_builder()
            .render_context_from(base)
            .fulfill_from(base)
            .build()
    }

    /// Build the base Render Environment used while resolving this pack's
    /// external package requirements.
    pub fn preparation_environment(&self) -> Result<RenderEnvironment, RenderEnvironmentError> {
        RenderEnvironment::builder()
            .package_bundles(self.package_bundles.bundles().iter().cloned())
            .font_set(self.font_set())
            .build()
    }

    /// Build a render-ready environment after verifying exact external package trees.
    pub fn render_environment_with_external_packages(
        &self,
        bundles: impl IntoIterator<Item = PackageBundle>,
    ) -> Result<RenderEnvironment, ProjectPackError> {
        self.environment_builder().package_bundles(bundles).build()
    }

    fn font_containers<'a>(
        &self,
        external: impl IntoIterator<Item = &'a Vec<u8>>,
    ) -> std::collections::HashMap<[u8; 16], Vec<u8>> {
        let mut containers = std::collections::HashMap::new();
        for data in &self.font_files {
            containers.insert(
                typst_pack::FontContainerIdentity::from_bytes(data).digest(),
                data.clone(),
            );
        }
        for data in external {
            containers.insert(
                typst_pack::FontContainerIdentity::from_bytes(data).digest(),
                data.clone(),
            );
        }
        containers
    }
}

/// Builds a render environment whose document resources are exactly those
/// declared by a [`ProjectPack`].
#[derive(Clone, Debug)]
pub struct ProjectPackEnvironmentBuilder<'a> {
    pack: &'a ProjectPack,
    package_bundles: Vec<PackageBundle>,
    font_files: Vec<Vec<u8>>,
    render_date: RenderDate,
    inputs: Dict,
}

impl ProjectPackEnvironmentBuilder<'_> {
    /// Copy only Render Context from an existing environment.
    pub fn render_context_from(mut self, environment: &RenderEnvironment) -> Self {
        self.render_date = environment.render_date();
        self.inputs = environment.inputs().clone();
        self
    }

    /// Use an explicit Render Date.
    pub fn render_date(mut self, render_date: RenderDate) -> Self {
        self.render_date = render_date;
        self
    }

    /// Replace the Typst values visible through `sys.inputs`.
    pub fn inputs(mut self, inputs: Dict) -> Self {
        self.inputs = inputs;
        self
    }

    /// Add or replace one Typst value visible through `sys.inputs`.
    pub fn input(mut self, key: impl Into<String>, value: impl IntoValue) -> Self {
        self.inputs.insert(key.into().into(), value.into_value());
        self
    }

    /// Use an existing environment to fulfill declared external resources.
    /// Undeclared packages and fonts are ignored.
    pub fn fulfill_from(mut self, environment: &RenderEnvironment) -> Self {
        for requirement in &self.pack.external_package_requirements {
            if let Some(bundle) = environment.package_bundle(&requirement.spec) {
                self = self.package_bundle(bundle.clone());
            }
        }
        let required_fonts = self
            .pack
            .external_font_requirements
            .iter()
            .map(|requirement| requirement.container_digest)
            .collect::<std::collections::HashSet<_>>();
        self.font_files
            .extend(environment.font_set().container_files_where(|data| {
                required_fonts
                    .contains(&typst_pack::FontContainerIdentity::from_bytes(data).digest())
            }));
        self
    }

    /// Supply an exact external Package Bundle.
    pub fn package_bundle(mut self, bundle: PackageBundle) -> Self {
        if let Some(existing) = self
            .package_bundles
            .iter_mut()
            .find(|existing| existing.spec() == bundle.spec())
        {
            *existing = bundle;
        } else {
            self.package_bundles.push(bundle);
        }
        self
    }

    /// Supply exact external Package Bundles.
    pub fn package_bundles(mut self, bundles: impl IntoIterator<Item = PackageBundle>) -> Self {
        for bundle in bundles {
            self = self.package_bundle(bundle);
        }
        self
    }

    /// Supply an exact external font container.
    pub fn font_file(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.font_files.push(bytes.into());
        self
    }

    /// Supply exact external font containers.
    pub fn font_files(mut self, files: impl IntoIterator<Item = impl Into<Vec<u8>>>) -> Self {
        self.font_files.extend(files.into_iter().map(Into::into));
        self
    }

    /// Build the exact render environment.
    pub fn build(self) -> Result<RenderEnvironment, ProjectPackError> {
        if let Some(bundle) = self.package_bundles.iter().find(|bundle| {
            !self
                .pack
                .external_package_requirements
                .iter()
                .any(|requirement| requirement.spec == *bundle.spec())
        }) {
            return Err(ProjectPackError::UnexpectedExternalPackage {
                spec: bundle.spec().clone(),
            });
        }

        let supplied_packages =
            PackageBundleSet::from_bundles(self.package_bundles).map_err(|duplicate| {
                ProjectPackError::DuplicatePackage {
                    spec: duplicate.spec,
                }
            })?;
        verify_external_package_requirements(&self.pack.external_package_requirements, |spec| {
            supplied_packages.get(spec)
        })?;

        let containers = self.pack.font_containers(&self.font_files);
        for requirement in &self.pack.external_font_requirements {
            let Some(data) = containers.get(&requirement.container_digest) else {
                return Err(ProjectPackError::MissingExternalFont {
                    container_digest: requirement.container_digest,
                });
            };
            if data.len() as u64 != requirement.container_length {
                return Err(ProjectPackError::MismatchedExternalFont {
                    container_digest: requirement.container_digest,
                });
            }
        }
        let mut faces = Vec::new();
        for face in &self.pack.font_catalog {
            let data = containers.get(&face.container_digest).ok_or_else(|| {
                ProjectPackError::MissingExternalFont {
                    container_digest: face.container_digest,
                }
            })?;
            if typst::text::FontInfo::new(data, face.index).is_none() {
                return Err(ProjectPackError::MismatchedExternalFont {
                    container_digest: face.container_digest,
                });
            }
            faces.push((data.clone(), face.index));
        }

        RenderEnvironment::builder()
            .package_bundles(
                self.pack
                    .package_bundles
                    .bundles()
                    .iter()
                    .cloned()
                    .chain(supplied_packages.bundles().iter().cloned()),
            )
            .font_set(FontSet::from_font_faces(faces))
            .render_date(self.render_date)
            .inputs(self.inputs)
            .build()
            .map_err(ProjectPackError::Environment)
    }
}

/// Builder for a [`ProjectPack`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPackBuilder {
    project: Project,
    package_bundles: Vec<PackageBundle>,
    external_package_bundles: Vec<PackageBundle>,
    font_faces: Vec<ProjectPackBuilderFontFace>,
    invalid_font: bool,
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

    /// Record a complete Package Bundle as an external package tree without
    /// vendoring its files.
    ///
    /// The files provide the exact tree identity required by the pack format.
    /// They are inspected while writing but are not stored in the archive.
    pub fn external_package_bundle(mut self, bundle: PackageBundle) -> Self {
        self.external_package_bundles.push(bundle);
        self
    }

    /// Embed a font file; collections contribute every face.
    pub fn font_file(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self = self.font_container(bytes.into(), true);
        self
    }

    /// Record a complete font container as external without embedding its bytes.
    pub fn external_font_file(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self = self.font_container(bytes.into(), false);
        self
    }

    /// Embed one face from a font container at this position in the Font Catalog.
    pub fn font_face(mut self, bytes: impl Into<Vec<u8>>, index: u32) -> Self {
        self = self.push_font_face(bytes.into(), index, true);
        self
    }

    /// Record one external face at this position in the Font Catalog.
    pub fn external_font_face(mut self, bytes: impl Into<Vec<u8>>, index: u32) -> Self {
        self = self.push_font_face(bytes.into(), index, false);
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
        let external_package_bundles =
            PackageBundleSet::from_bundles(self.external_package_bundles).map_err(|duplicate| {
                ProjectPackError::DuplicatePackage {
                    spec: duplicate.spec,
                }
            })?;

        for bundle in external_package_bundles.bundles() {
            if package_bundles.get(bundle.spec()).is_some() {
                return Err(ProjectPackError::DuplicatePackage {
                    spec: bundle.spec().clone(),
                });
            }
            external_package_requirement(bundle)?;
        }

        if self.invalid_font {
            return Err(ProjectPackError::UnrecognizedFont);
        }

        let pack = ProjectPack {
            project: self.project,
            package_bundles,
            external_packages: external_package_bundles
                .bundles()
                .iter()
                .map(|bundle| bundle.spec().clone())
                .collect(),
            external_package_requirements: Vec::new(),
            external_package_bundles: external_package_bundles.bundles().to_vec(),
            font_catalog: Vec::new(),
            external_font_requirements: Vec::new(),
            font_files: Vec::new(),
            builder_font_faces: self.font_faces,
            metadata: self.metadata,
            source_bytes: None,
        };
        let bytes = pack.to_bytes()?;

        ProjectPack::from_bytes(bytes)
    }

    fn font_container(mut self, bytes: Vec<u8>, embedded: bool) -> Self {
        let indices = (0..)
            .take_while(|index| typst::text::FontInfo::new(&bytes, *index).is_some())
            .collect::<Vec<_>>();
        if indices.is_empty() {
            self.invalid_font = true;
        }
        for index in indices {
            self.font_faces.push(ProjectPackBuilderFontFace {
                data: bytes.clone(),
                index,
                embedded,
            });
        }
        self
    }

    fn push_font_face(mut self, bytes: Vec<u8>, index: u32, embedded: bool) -> Self {
        if typst::text::FontInfo::new(&bytes, index).is_none() {
            self.invalid_font = true;
        } else {
            self.font_faces.push(ProjectPackBuilderFontFace {
                data: bytes,
                index,
                embedded,
            });
        }
        self
    }
}

/// An exact Complete Package Tree that must be fulfilled outside a Project Pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalPackageRequirement {
    spec: PackageSpec,
    tree_digest: [u8; 16],
    file_count: u64,
    byte_length: u64,
}

impl ExternalPackageRequirement {
    /// Return the exact package spec.
    pub fn spec(&self) -> &PackageSpec {
        &self.spec
    }

    /// Return the canonical Complete Package Tree digest.
    pub fn tree_digest(&self) -> [u8; 16] {
        self.tree_digest
    }

    /// Return the number of files in the required package tree.
    pub fn file_count(&self) -> u64 {
        self.file_count
    }

    /// Return the total byte length of the required package tree.
    pub fn byte_length(&self) -> u64 {
        self.byte_length
    }
}

/// An exact font container that must be fulfilled outside a Project Pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalFontRequirement {
    container_digest: [u8; 16],
    container_length: u64,
    face_indices: Vec<u32>,
}

impl ExternalFontRequirement {
    /// Return the canonical font-container digest.
    pub fn container_digest(&self) -> [u8; 16] {
        self.container_digest
    }

    /// Return the exact container byte length.
    pub fn container_length(&self) -> u64 {
        self.container_length
    }

    /// Return the required face indices within the container.
    pub fn face_indices(&self) -> &[u32] {
        &self.face_indices
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProjectPackFontFace {
    container_digest: [u8; 16],
    index: u32,
    embedded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProjectPackBuilderFontFace {
    data: Vec<u8>,
    index: u32,
    embedded: bool,
}

fn external_package_requirement(
    bundle: &PackageBundle,
) -> Result<ExternalPackageRequirement, ProjectPackError> {
    let archive_error = |error: &dyn std::fmt::Display| ProjectPackError::Archive {
        message: error.to_string(),
    };
    let mut pack = typst_pack::Pack::builder("main.typ")
        .file("main.typ", Vec::new())
        .map_err(|error| archive_error(&error))?;
    for (path, data) in bundle.files() {
        pack = pack
            .external_package_file(bundle.spec().clone(), path, data)
            .map_err(|error| archive_error(&error))?;
    }
    let pack = pack.build().map_err(|error| archive_error(&error))?;
    let requirement = pack.package_requirements().first().ok_or_else(|| {
        ProjectPackError::EmptyExternalPackage {
            spec: bundle.spec().clone(),
        }
    })?;

    Ok(ExternalPackageRequirement {
        spec: requirement.spec().clone(),
        tree_digest: requirement.tree_identity().digest(),
        file_count: requirement.file_count(),
        byte_length: requirement.byte_length(),
    })
}

fn verify_external_package_requirements<'a>(
    requirements: &[ExternalPackageRequirement],
    mut get: impl FnMut(&PackageSpec) -> Option<&'a PackageBundle>,
) -> Result<(), ProjectPackError> {
    for expected in requirements {
        let bundle =
            get(&expected.spec).ok_or_else(|| ProjectPackError::MissingExternalPackage {
                spec: expected.spec.clone(),
            })?;
        if external_package_requirement(bundle)? != *expected {
            return Err(ProjectPackError::MismatchedExternalPackage {
                spec: expected.spec.clone(),
            });
        }
    }
    Ok(())
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
    Project(ProjectValidationError),

    /// A vendored package could not be converted into a Package Bundle.
    Package {
        spec: String,
        error: PackageBundleError,
    },

    /// More than one vendored Package Bundle has the same exact package spec.
    DuplicatePackage { spec: PackageSpec },

    /// An embedded font file could not be parsed as a font.
    UnrecognizedFont,

    /// An external Package Bundle contains no files and cannot establish a tree identity.
    EmptyExternalPackage { spec: PackageSpec },

    /// An exact external package requirement is not present in the Render Environment.
    MissingExternalPackage { spec: PackageSpec },

    /// A Package Bundle does not match the pack's exact external tree requirement.
    MismatchedExternalPackage { spec: PackageSpec },

    /// A Package Bundle was supplied that the pack does not require.
    UnexpectedExternalPackage { spec: PackageSpec },

    /// An exact external font container is unavailable.
    MissingExternalFont { container_digest: [u8; 16] },

    /// A supplied font container does not match the pack's exact requirement.
    MismatchedExternalFont { container_digest: [u8; 16] },

    /// The pack's Render Environment could not be built.
    Environment(RenderEnvironmentError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pack() -> ProjectPack {
        let project = Project::builder("main.typ")
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
            .external_package_bundle(
                PackageBundle::builder(
                    "@preview/cetz:0.4.2"
                        .parse()
                        .expect("external spec should parse"),
                )
                .file("typst.toml", b"[package]".to_vec())
                .file("lib.typ", b"".to_vec())
                .build()
                .expect("external bundle should be valid"),
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
        let raw = typst_pack::Pack::from_bytes(bytes.clone()).expect("raw pack should parse");
        let external = raw
            .package_requirements()
            .iter()
            .find(|requirement| !requirement.is_embedded())
            .expect("external package requirement should be recorded");
        assert_eq!(external.spec().to_string(), "@preview/cetz:0.4.2");
        assert_eq!(external.file_count(), 2);
        assert!(!raw.has_package(external.spec()));

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
        assert_eq!(
            read.to_bytes().expect("loaded pack should serialize"),
            bytes
        );
    }

    #[test]
    fn project_pack_embeds_and_restores_font_files() {
        let font = typst_assets::fonts()
            .next()
            .expect("bundled fonts should not be empty")
            .to_vec();
        let pack = ProjectPack::builder(Project::from_source("Hello"))
            .font_file(font.clone())
            .build()
            .expect("pack with a font should build");

        let bytes = pack.to_bytes().expect("pack should serialize");
        let read = ProjectPack::from_bytes(&bytes).expect("pack should parse back");

        assert_eq!(read.font_files(), std::slice::from_ref(&font));
        assert_eq!(read.font_set().container_files_where(|_| true), [font]);
    }

    #[test]
    fn project_pack_render_environment_installs_vendored_bundles() {
        let environment = sample_pack()
            .preparation_environment()
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
    fn project_pack_verifies_exact_external_package_trees() {
        let pack = sample_pack();
        let matching = PackageBundle::builder(
            "@preview/cetz:0.4.2"
                .parse()
                .expect("external spec should parse"),
        )
        .file("typst.toml", b"[package]".to_vec())
        .file("lib.typ", b"".to_vec())
        .build()
        .expect("matching bundle should build");
        assert!(matches!(
            pack.render_environment(),
            Err(ProjectPackError::MissingExternalPackage { .. })
        ));

        let matching_environment = pack
            .preparation_environment()
            .expect("base environment should build")
            .to_builder()
            .package_bundle(matching)
            .build()
            .expect("matching environment should build");
        pack.verify_external_packages(&matching_environment)
            .expect("matching package tree should verify");
        pack.render_environment_with_external_packages([matching_environment
            .package_bundle(
                &"@preview/cetz:0.4.2"
                    .parse()
                    .expect("external spec should parse"),
            )
            .expect("matching bundle should be installed")
            .clone()])
            .expect("verified render environment should build");

        let unexpected_spec: PackageSpec = "@demo/unexpected:0.1.0"
            .parse()
            .expect("unexpected spec should parse");
        let unexpected = PackageBundle::builder(unexpected_spec.clone())
            .file("lib.typ", b"".to_vec())
            .build()
            .expect("unexpected bundle should build");
        let render_date = RenderDate::from_ymd(2030, 2, 3).expect("date should be valid");
        let base = matching_environment
            .to_builder()
            .package_bundle(unexpected.clone())
            .render_date(render_date)
            .input("tenant", "demo")
            .build()
            .expect("base environment should build");
        let rendered = pack
            .render_environment_from(&base)
            .expect("base should fulfill the pack");
        assert_eq!(rendered.render_date(), render_date);
        assert_eq!(rendered.inputs(), base.inputs());
        assert!(rendered.package_bundle(&unexpected_spec).is_none());

        assert!(matches!(
            pack.render_environment_with_external_packages([unexpected]),
            Err(ProjectPackError::UnexpectedExternalPackage { .. })
        ));

        let mismatched = PackageBundle::builder(
            "@preview/cetz:0.4.2"
                .parse()
                .expect("external spec should parse"),
        )
        .file("typst.toml", b"[package]".to_vec())
        .file("lib.typ", b"changed".to_vec())
        .build()
        .expect("mismatched bundle should build");
        let mismatched_environment = pack
            .preparation_environment()
            .expect("base environment should build")
            .to_builder()
            .package_bundle(mismatched)
            .build()
            .expect("mismatched environment should build");
        assert_eq!(
            pack.verify_external_packages(&mismatched_environment),
            Err(ProjectPackError::MismatchedExternalPackage {
                spec: "@preview/cetz:0.4.2"
                    .parse()
                    .expect("external spec should parse"),
            })
        );
    }

    #[test]
    fn project_pack_rejects_garbage_bytes() {
        let result = ProjectPack::from_bytes(b"not a pack");

        assert!(matches!(result, Err(ProjectPackError::Archive { .. })));
    }

    #[test]
    fn project_pack_fulfills_external_fonts_from_a_base_environment() {
        let external_font = typst_assets::fonts()
            .next()
            .expect("bundled fonts should not be empty")
            .to_vec();
        let embedded_font = typst_assets::fonts()
            .nth(1)
            .expect("bundled fonts should contain a second font")
            .to_vec();
        let ambient_font = typst_assets::fonts()
            .nth(2)
            .expect("bundled fonts should contain a third font")
            .to_vec();
        let pack = ProjectPack::builder(Project::from_source("Hello"))
            .external_font_face(external_font.clone(), 0)
            .font_face(embedded_font.clone(), 0)
            .build()
            .expect("pack with an external font should build");

        assert_eq!(pack.external_font_requirements().len(), 1);
        let raw = typst_pack::Pack::from_bytes(pack.to_bytes().expect("pack should serialize"))
            .expect("raw pack should parse");
        assert!(!raw.font_catalog()[0].is_embedded());
        assert!(raw.font_catalog()[1].is_embedded());
        assert!(matches!(
            pack.render_environment(),
            Err(ProjectPackError::MissingExternalFont { .. })
        ));

        let base = RenderEnvironment::builder()
            .font_set(FontSet::from_font_files([
                ambient_font,
                external_font.clone(),
            ]))
            .build()
            .expect("base environment should build");
        let environment = pack
            .render_environment_from(&base)
            .expect("base font should fulfill the pack");

        let store = environment.font_set().font_store();
        assert_eq!(
            store.font(0).expect("external face").data().as_slice(),
            external_font
        );
        assert_eq!(
            store.font(1).expect("embedded face").data().as_slice(),
            embedded_font
        );
        assert!(store.font(2).is_none());
    }

    #[test]
    fn project_pack_builder_rejects_unrecognized_fonts_and_duplicate_packages() {
        let font_result = ProjectPack::builder(Project::from_source("Hello"))
            .font_file(b"not a font".to_vec())
            .build();
        assert_eq!(font_result, Err(ProjectPackError::UnrecognizedFont));

        let empty_external = PackageBundle::builder(
            "@demo/empty:0.1.0"
                .parse()
                .expect("external spec should parse"),
        )
        .build()
        .expect("empty package bundle should build");
        assert_eq!(
            ProjectPack::builder(Project::from_source("Hello"))
                .external_package_bundle(empty_external)
                .build(),
            Err(ProjectPackError::EmptyExternalPackage {
                spec: "@demo/empty:0.1.0"
                    .parse()
                    .expect("external spec should parse"),
            })
        );

        let bundle = |spec: &str| {
            PackageBundle::builder(spec.parse().expect("spec should parse"))
                .file("lib.typ", b"".to_vec())
                .build()
                .expect("bundle should build")
        };
        let duplicate_result = ProjectPack::builder(Project::from_source("Hello"))
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
