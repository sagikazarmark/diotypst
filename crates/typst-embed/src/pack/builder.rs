use super::error::ProjectPackError;
use super::metadata::ProjectPackMetadata;
use super::requirements::external_package_requirement;
use super::{ProjectPack, ProjectPackBuilderFontFace};
use crate::{PackageBundle, PackageBundleSet, Project};

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
    pub(super) fn new(project: Project) -> Self {
        Self {
            project,
            package_bundles: Vec::new(),
            external_package_bundles: Vec::new(),
            font_faces: Vec::new(),
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
