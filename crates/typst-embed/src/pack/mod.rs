mod builder;
mod environment;
mod error;
mod metadata;
mod requirements;

#[cfg(test)]
mod tests;

pub use builder::ProjectPackBuilder;
pub use environment::ProjectPackEnvironmentBuilder;
pub use error::ProjectPackError;
pub use metadata::ProjectPackMetadata;
pub use requirements::{ExternalFontRequirement, ExternalPackageRequirement};

use crate::{
    FontSet, PackageBundle, PackageBundleSet, PackageSpec, Project, RenderEnvironment,
    RenderEnvironmentError,
};
use requirements::verify_external_package_requirements;

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
        ProjectPackBuilder::new(project)
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
            .map(|requirement| {
                ExternalPackageRequirement::new(
                    requirement.spec().clone(),
                    requirement.tree_identity().digest(),
                    requirement.file_count(),
                    requirement.byte_length(),
                )
            })
            .collect::<Vec<_>>();
        let external_packages = external_package_requirements
            .iter()
            .map(|requirement| requirement.spec().clone())
            .collect();

        let external_font_requirements = pack
            .font_requirements()
            .iter()
            .filter(|requirement| !requirement.is_embedded())
            .map(|requirement| {
                ExternalFontRequirement::new(
                    requirement.container_identity().digest(),
                    requirement.container_length(),
                    requirement.face_indices().to_vec(),
                )
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
            if let Some(name) = metadata.name() {
                converted = converted.with_name(name);
            }
            if let Some(description) = metadata.description() {
                converted = converted.with_description(description);
            }
            for author in metadata.authors() {
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
        ProjectPackEnvironmentBuilder::new(self)
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
