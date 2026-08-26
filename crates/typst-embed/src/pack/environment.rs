use super::ProjectPack;
use super::error::ProjectPackError;
use super::requirements::verify_external_package_requirements;
use crate::{FontSet, PackageBundle, PackageBundleSet, RenderDate, RenderEnvironment};
use typst::foundations::{Dict, IntoValue};

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

impl<'a> ProjectPackEnvironmentBuilder<'a> {
    pub(super) fn new(pack: &'a ProjectPack) -> Self {
        Self {
            pack,
            package_bundles: Vec::new(),
            font_files: Vec::new(),
            render_date: RenderDate::default(),
            inputs: Dict::new(),
        }
    }

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
            if let Some(bundle) = environment.package_bundle(requirement.spec()) {
                self = self.package_bundle(bundle.clone());
            }
        }
        let required_fonts = self
            .pack
            .external_font_requirements
            .iter()
            .map(|requirement| requirement.container_digest())
            .collect::<std::collections::HashSet<_>>();
        self.font_files
            .extend(environment.font_set().container_files_where(|data| {
                required_fonts.contains(
                    &typst_pack::CanonicalIdentity::for_font_container_bytes(data).digest(),
                )
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
                .any(|requirement| requirement.spec() == bundle.spec())
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
            let Some(data) = containers.get(&requirement.container_digest()) else {
                return Err(ProjectPackError::MissingExternalFont {
                    container_digest: requirement.container_digest(),
                });
            };
            if data.len() as u64 != requirement.container_length() {
                return Err(ProjectPackError::MismatchedExternalFont {
                    container_digest: requirement.container_digest(),
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
