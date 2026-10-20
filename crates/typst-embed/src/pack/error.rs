use crate::{PackageBundleError, PackageSpec, ProjectValidationError, RenderEnvironmentError};

/// A Project Pack read, validation, or write failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ProjectPackError {
    /// The bytes could not be read or written as a `.typk` archive.
    #[error("the bytes could not be read or written as a `.typk` archive: {message}")]
    Archive {
        /// What the archive format reported.
        message: String,
    },

    /// The packed files do not form a valid Typst Project.
    #[error("the packed files do not form a valid typst project")]
    Project(#[source] ProjectValidationError),

    /// A vendored package could not be converted into a Package Bundle.
    #[error("vendored package {spec} could not be read as a package bundle")]
    Package {
        /// The exact package spec that could not be converted.
        spec: String,
        /// Why the package bundle was rejected.
        #[source]
        error: PackageBundleError,
    },

    /// More than one vendored Package Bundle has the same exact package spec.
    #[error("more than one vendored package bundle has the exact spec {spec}")]
    DuplicatePackage {
        /// The exact package spec that appeared more than once.
        spec: PackageSpec,
    },

    /// A font file could not be parsed as a font.
    #[error("a font file could not be parsed as a font")]
    UnrecognizedFont,

    /// An external Package Bundle contains no files and cannot establish a tree identity.
    #[error("external package {spec} contains no files, so it has no package-tree identity")]
    EmptyExternalPackage {
        /// The exact package spec of the empty bundle.
        spec: PackageSpec,
    },

    /// An exact external package requirement is not present in the Render Environment.
    #[error("the pack requires external package {spec}, which the render environment lacks")]
    MissingExternalPackage {
        /// The exact package spec the pack requires.
        spec: PackageSpec,
    },

    /// A Package Bundle does not match the pack's exact external tree requirement.
    #[error("the supplied bundle for {spec} does not match the pack's exact package tree")]
    MismatchedExternalPackage {
        /// The exact package spec whose tree did not match.
        spec: PackageSpec,
    },

    /// A Package Bundle was supplied that the pack does not require.
    #[error("package bundle {spec} was supplied, but the pack does not require it")]
    UnexpectedExternalPackage {
        /// The exact package spec that was supplied but not required.
        spec: PackageSpec,
    },

    /// An exact external font container is unavailable.
    #[error(
        "the pack requires an external font container ({}), which is unavailable",
        families.join(", ")
    )]
    MissingExternalFont {
        /// The SHA-256 digest of the required font container.
        container_digest: [u8; 32],
        /// The family names of the required container's faces.
        families: Vec<String>,
    },

    /// The pack's Render Environment could not be built.
    #[error("the pack's render environment could not be built")]
    Environment(#[source] RenderEnvironmentError),
}
