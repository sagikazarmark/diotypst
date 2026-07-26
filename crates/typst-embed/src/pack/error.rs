use crate::{PackageBundleError, PackageSpec, ProjectValidationError, RenderEnvironmentError};

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
