use crate::paths::parse_file_path;
use std::collections::HashSet;
use typst_syntax::VirtualPath;
use typst_syntax::package::PackageSpec;

/// A set of package files identified by an exact Typst package spec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageBundle {
    spec: PackageSpec,
    files: Vec<PackageFile>,
}

impl PackageBundle {
    /// Start building a package bundle for the given exact package spec.
    pub fn builder(spec: PackageSpec) -> PackageBundleBuilder {
        PackageBundleBuilder {
            spec,
            files: Vec::new(),
        }
    }

    /// Return this bundle's exact package spec.
    pub fn spec(&self) -> &PackageSpec {
        &self.spec
    }

    /// Return this bundle's files as (package-internal path, bytes) pairs.
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .map(|file| (file.path.get_without_slash(), file.bytes.as_slice()))
    }

    /// Return bytes for a package file by package-internal path.
    pub fn file_bytes(&self, path: impl AsRef<str>) -> Option<&[u8]> {
        let path = VirtualPath::new(path.as_ref()).ok()?;

        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.bytes.as_slice())
    }
}

/// Builder for a package bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageBundleBuilder {
    spec: PackageSpec,
    files: Vec<(String, Vec<u8>)>,
}

impl PackageBundleBuilder {
    /// Add a package file to this bundle.
    pub fn file(mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.push((path.into(), bytes.into()));
        self
    }

    /// Build and validate the package bundle.
    pub fn build(self) -> Result<PackageBundle, PackageBundleError> {
        let files = self
            .files
            .into_iter()
            .map(|(path, bytes)| {
                Ok(PackageFile {
                    path: parse_file_path(&path).ok_or(PackageBundleError::InvalidPath { path })?,
                    bytes,
                })
            })
            .collect::<Result<Vec<_>, PackageBundleError>>()?;

        let mut paths = HashSet::new();
        for file in &files {
            if !paths.insert(file.path.get_without_slash()) {
                return Err(PackageBundleError::DuplicatePath {
                    path: file.path.get_without_slash().to_owned(),
                });
            }
        }

        Ok(PackageBundle {
            spec: self.spec,
            files,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackageFile {
    path: VirtualPath,
    bytes: Vec<u8>,
}

/// A spec-unique collection of Package Bundles.
///
/// Every holder of Package Bundles needs the same find-by-spec, add-or-replace, and
/// duplicate-spec-rejection semantics; this type owns them so spec-collision behavior
/// cannot drift between holders.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PackageBundleSet {
    bundles: Vec<PackageBundle>,
}

/// More than one Package Bundle carried the same exact package spec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicatePackageSpec {
    /// The exact package spec that appeared more than once.
    pub spec: PackageSpec,
}

impl PackageBundleSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect bundles, rejecting duplicate exact specs.
    pub fn from_bundles(
        bundles: impl IntoIterator<Item = PackageBundle>,
    ) -> Result<Self, DuplicatePackageSpec> {
        let mut set = Self::new();
        for bundle in bundles {
            set.try_insert(bundle)?;
        }

        Ok(set)
    }

    /// Return the bundle with the given exact spec.
    pub fn get(&self, spec: &PackageSpec) -> Option<&PackageBundle> {
        self.bundles.iter().find(|bundle| bundle.spec() == spec)
    }

    /// Add a bundle, replacing any existing bundle with the same exact spec.
    pub fn upsert(&mut self, bundle: PackageBundle) {
        match self
            .bundles
            .iter_mut()
            .find(|existing| existing.spec() == bundle.spec())
        {
            Some(existing) => *existing = bundle,
            None => self.bundles.push(bundle),
        }
    }

    /// Add a bundle, rejecting an existing bundle with the same exact spec.
    pub fn try_insert(&mut self, bundle: PackageBundle) -> Result<(), DuplicatePackageSpec> {
        if self.get(bundle.spec()).is_some() {
            return Err(DuplicatePackageSpec {
                spec: bundle.spec().clone(),
            });
        }
        self.bundles.push(bundle);

        Ok(())
    }

    /// Return the bundles in insertion order.
    pub fn bundles(&self) -> &[PackageBundle] {
        &self.bundles
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PackageBundle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct PackageFileWire<'a> {
            path: &'a str,
            bytes: &'a [u8],
        }

        #[derive(serde::Serialize)]
        struct PackageBundleWire<'a> {
            spec: String,
            files: Vec<PackageFileWire<'a>>,
        }

        PackageBundleWire {
            spec: self.spec.to_string(),
            files: self
                .files
                .iter()
                .map(|file| PackageFileWire {
                    path: file.path.get_without_slash(),
                    bytes: &file.bytes,
                })
                .collect(),
        }
        .serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PackageBundle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use std::str::FromStr;

        #[derive(serde::Deserialize)]
        struct PackageFileFields {
            path: String,
            bytes: Vec<u8>,
        }

        #[derive(serde::Deserialize)]
        struct PackageBundleFields {
            spec: String,
            files: Vec<PackageFileFields>,
        }

        let fields = <PackageBundleFields as serde::Deserialize>::deserialize(deserializer)?;
        let spec = PackageSpec::from_str(&fields.spec).map_err(|_| {
            serde::de::Error::custom(format!("invalid exact package spec: {}", fields.spec))
        })?;

        let mut builder = PackageBundle::builder(spec);
        for file in fields.files {
            builder = builder.file(file.path, file.bytes);
        }

        builder
            .build()
            .map_err(|error| serde::de::Error::custom(format!("invalid Package Bundle: {error:?}")))
    }
}

/// A package bundle validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageBundleError {
    /// A package file path does not name a file inside the package bundle.
    InvalidPath { path: String },

    /// More than one package file has the same package-internal path.
    DuplicatePath { path: String },
}
