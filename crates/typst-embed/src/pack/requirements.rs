use super::error::ProjectPackError;
use crate::{PackageBundle, PackageSpec};

/// An exact Complete Package Tree that must be fulfilled outside a Project Pack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalPackageRequirement {
    spec: PackageSpec,
    tree_digest: [u8; 16],
    file_count: u64,
    byte_length: u64,
}

impl ExternalPackageRequirement {
    pub(super) fn new(
        spec: PackageSpec,
        tree_digest: [u8; 16],
        file_count: u64,
        byte_length: u64,
    ) -> Self {
        Self {
            spec,
            tree_digest,
            file_count,
            byte_length,
        }
    }

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
    pub(super) fn new(
        container_digest: [u8; 16],
        container_length: u64,
        face_indices: Vec<u32>,
    ) -> Self {
        Self {
            container_digest,
            container_length,
            face_indices,
        }
    }

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

pub(super) fn external_package_requirement(
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

pub(super) fn verify_external_package_requirements<'a>(
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
