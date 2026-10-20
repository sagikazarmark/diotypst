use super::error::ProjectPackError;
use super::metadata::ProjectPackMetadata;
use super::requirements::package_tree_digest;
use super::{ProjectPack, ProjectPackBuilderFont};
use crate::{PackageBundle, PackageBundleSet, Project};

/// Builder for a [`ProjectPack`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPackBuilder {
    project: Project,
    package_bundles: Vec<PackageBundle>,
    external_package_bundles: Vec<PackageBundle>,
    fonts: Vec<ProjectPackBuilderFont>,
    invalid_font: bool,
    metadata: Option<ProjectPackMetadata>,
}

impl ProjectPackBuilder {
    pub(super) fn new(project: Project) -> Self {
        Self {
            project,
            package_bundles: Vec::new(),
            external_package_bundles: Vec::new(),
            fonts: Vec::new(),
            invalid_font: false,
            metadata: None,
        }
    }

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

    /// Embed a font container (a font file or collection) at this position in
    /// the pack's font order; every face it holds becomes available.
    ///
    /// A container already added to the builder keeps its first position and
    /// disposition.
    pub fn font_file(self, bytes: impl Into<Vec<u8>>) -> Self {
        self.font_container(bytes.into(), true)
    }

    /// Record a complete font container as external, at this position in the
    /// pack's font order, without embedding its bytes.
    ///
    /// The bytes provide the exact container identity required by the pack
    /// format. A container already added to the builder keeps its first
    /// position and disposition.
    pub fn external_font_file(self, bytes: impl Into<Vec<u8>>) -> Self {
        self.font_container(bytes.into(), false)
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
            package_tree_digest(bundle)?;
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
            fonts: Vec::new(),
            external_font_requirements: Vec::new(),
            font_files: Vec::new(),
            builder_fonts: self.fonts,
            metadata: self.metadata,
            source_bytes: None,
        };
        let bytes = pack.to_bytes()?;

        ProjectPack::from_bytes(bytes)
    }

    fn font_container(mut self, data: Vec<u8>, embedded: bool) -> Self {
        if typst::text::FontInfo::iter(&data).next().is_none() {
            self.invalid_font = true;
        } else if !self.fonts.iter().any(|font| font.data == data) {
            self.fonts.push(ProjectPackBuilderFont { data, embedded });
        }
        self
    }
}
