use super::error::ProjectPackError;
use crate::{PackageBundle, PackageSpec};

/// An exact Complete Package Tree that must be fulfilled outside a Project Pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalPackageRequirement {
    spec: PackageSpec,
    tree_digest: [u8; 32],
}

impl ExternalPackageRequirement {
    pub(super) fn new(spec: PackageSpec, tree_digest: [u8; 32]) -> Self {
        Self { spec, tree_digest }
    }

    /// Return the exact package spec.
    pub fn spec(&self) -> &PackageSpec {
        &self.spec
    }

    /// Return the SHA-256 package tree digest, as defined by the `.typk`
    /// format.
    pub fn tree_digest(&self) -> [u8; 32] {
        self.tree_digest
    }
}

/// An exact font container that must be fulfilled outside a Project Pack.
///
/// The whole container is required: every face it holds is available to the
/// document once it is supplied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalFontRequirement {
    container_digest: [u8; 32],
    families: Vec<String>,
    face_indices: Vec<u32>,
}

impl ExternalFontRequirement {
    pub(super) fn new(
        container_digest: [u8; 32],
        families: Vec<String>,
        face_indices: Vec<u32>,
    ) -> Self {
        Self {
            container_digest,
            families,
            face_indices,
        }
    }

    /// Return the SHA-256 digest of the exact container bytes.
    pub fn container_digest(&self) -> [u8; 32] {
        self.container_digest
    }

    /// Return the distinct family names of the container's faces, in face
    /// order.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// Return the face indices within the container.
    pub fn face_indices(&self) -> &[u32] {
        &self.face_indices
    }
}

/// Compute the `.typk` package tree digest of a Package Bundle.
pub(super) fn package_tree_digest(bundle: &PackageBundle) -> Result<[u8; 32], ProjectPackError> {
    if bundle.files().next().is_none() {
        return Err(ProjectPackError::EmptyExternalPackage {
            spec: bundle.spec().clone(),
        });
    }
    let tree = typst_pack::package::Tree::new(bundle.files()).map_err(|error| {
        ProjectPackError::Archive {
            message: error.to_string(),
        }
    })?;
    Ok(*tree.digest().as_bytes())
}

pub(super) fn verify_external_package_requirements<'a>(
    requirements: &[ExternalPackageRequirement],
    mut get: impl FnMut(&PackageSpec) -> Option<&'a PackageBundle>,
) -> Result<(), ProjectPackError> {
    for expected in requirements {
        let bundle =
            get(&expected.spec).ok_or_else(|| ProjectPackError::MissingExternalPackage {
                spec: expected.spec.clone(),
            })?;
        if package_tree_digest(bundle)? != expected.tree_digest {
            return Err(ProjectPackError::MismatchedExternalPackage {
                spec: expected.spec.clone(),
            });
        }
    }
    Ok(())
}

/// The SHA-256 digest of exact font container bytes.
pub(super) fn font_container_digest(data: &[u8]) -> [u8; 32] {
    *typst_pack::Digest::of(data).as_bytes()
}
