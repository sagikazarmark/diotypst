use std::collections::HashSet;
use typst::syntax::VirtualPath;
use typst_package_source::parse_file_path;

/// Parse a Typst path for use inside a workspace.
///
/// This accepts everything [`VirtualPath::new`] accepts except the bare root,
/// which cannot name a file.
pub(crate) fn parse_workspace_path(path: &str) -> Result<VirtualPath, WorkspaceValidationError> {
    parse_file_path(path).ok_or_else(|| WorkspaceValidationError::InvalidPath {
        path: path.to_owned(),
    })
}

/// A renderable Typst unit with one root entrypoint and explicit files.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentWorkspace {
    root_path: VirtualPath,
    files: Vec<WorkspaceFile>,
}

impl DocumentWorkspace {
    /// Create a Typst Project from a single Typst source string.
    pub fn from_source(source: impl Into<String>) -> Self {
        Self::from_source_file("main.typ", source).expect("default root path is valid")
    }

    /// Create a Typst Project from one Typst source file at a custom root entrypoint.
    pub fn from_source_file(
        root_path: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, WorkspaceValidationError> {
        let root_path = parse_workspace_path(&root_path.into())?;
        let root_file = WorkspaceFile::source(root_path.get_without_slash(), source)?;

        Self {
            root_path,
            files: vec![root_file],
        }
        .validated()
    }

    /// Create a Typst Project from checked Project Files.
    pub fn new(
        root_path: impl Into<String>,
        files: impl IntoIterator<Item = WorkspaceFile>,
    ) -> Result<Self, WorkspaceValidationError> {
        Self {
            root_path: parse_workspace_path(&root_path.into())?,
            files: files.into_iter().collect(),
        }
        .validated()
    }

    /// Start building a Typst Project with the provided root entrypoint.
    pub fn builder(root_path: impl Into<String>) -> DocumentWorkspaceBuilder {
        DocumentWorkspaceBuilder {
            root_path: root_path.into(),
            files: Vec::new(),
        }
    }

    /// Return the root Typst entrypoint path.
    pub fn root_path(&self) -> &VirtualPath {
        &self.root_path
    }

    /// Return bytes for a file in this Typst Project.
    pub fn file_bytes(&self, path: impl AsRef<str>) -> Option<&[u8]> {
        let path = VirtualPath::new(path.as_ref()).ok()?;

        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.bytes.as_slice())
    }

    /// Return the explicit files in this Typst Project.
    pub fn files(&self) -> &[WorkspaceFile] {
        &self.files
    }

    /// Return whether this Typst Project contains a file at the given path.
    pub fn contains_path(&self, path: impl AsRef<str>) -> bool {
        self.file_bytes(path).is_some()
    }

    /// Add or replace exact Project Files while preserving this project's root entrypoint.
    pub fn overlay_files(mut self, files: impl IntoIterator<Item = WorkspaceFile>) -> Self {
        for overlay_file in files {
            if let Some(file) = self
                .files
                .iter_mut()
                .find(|file| file.path == overlay_file.path)
            {
                *file = overlay_file;
            } else {
                self.files.push(overlay_file);
            }
        }

        self
    }

    /// Validate that this Typst Project can be rendered as a coherent unit.
    pub fn validate(&self) -> Result<(), WorkspaceValidationError> {
        let mut paths = HashSet::new();

        for file in &self.files {
            if !paths.insert(file.path.get_without_slash()) {
                return Err(WorkspaceValidationError::DuplicatePath {
                    path: file.path.get_without_slash().to_owned(),
                });
            }
        }

        if self
            .file_bytes(self.root_path.get_without_slash())
            .is_none()
        {
            return Err(WorkspaceValidationError::MissingRoot {
                root: self.root_path.get_without_slash().to_owned(),
            });
        }

        Ok(())
    }

    fn validated(self) -> Result<Self, WorkspaceValidationError> {
        self.validate()?;

        Ok(self)
    }
}

/// Builder for a Typst Project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentWorkspaceBuilder {
    root_path: String,
    files: Vec<(String, Vec<u8>)>,
}

impl DocumentWorkspaceBuilder {
    /// Add a UTF-8 Typst source file to this workspace.
    pub fn source_file(self, path: impl Into<String>, source: impl Into<String>) -> Self {
        self.file(path, source.into().into_bytes())
    }

    /// Add a binary Project File to this project.
    pub fn file(mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.files.push((path.into(), bytes.into()));
        self
    }

    /// Build and validate the Typst Project.
    pub fn build(self) -> Result<DocumentWorkspace, WorkspaceValidationError> {
        let root_path = parse_workspace_path(&self.root_path)?;
        let files = self
            .files
            .into_iter()
            .map(|(path, bytes)| {
                Ok(WorkspaceFile {
                    path: parse_workspace_path(&path)?,
                    bytes,
                })
            })
            .collect::<Result<Vec<_>, WorkspaceValidationError>>()?;

        let workspace = DocumentWorkspace { root_path, files };

        workspace.validate()?;

        Ok(workspace)
    }
}

/// A named byte resource inside a Typst Project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceFile {
    path: VirtualPath,
    bytes: Vec<u8>,
}

impl WorkspaceFile {
    /// Create a Project File from bytes.
    pub fn new(
        path: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<Self, WorkspaceValidationError> {
        Ok(Self {
            path: parse_workspace_path(&path.into())?,
            bytes: bytes.into(),
        })
    }

    /// Create a UTF-8 Typst source Project File.
    pub fn source(
        path: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<Self, WorkspaceValidationError> {
        Self::new(path, source.into().into_bytes())
    }

    /// Return this file's Project Path.
    pub fn path(&self) -> &VirtualPath {
        &self.path
    }

    /// Return this file's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for WorkspaceFile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct WorkspaceFileWire<'a> {
            path: &'a str,
            bytes: &'a [u8],
        }

        WorkspaceFileWire {
            path: self.path.get_without_slash(),
            bytes: &self.bytes,
        }
        .serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for WorkspaceFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct WorkspaceFileFields {
            path: String,
            bytes: Vec<u8>,
        }

        let fields = <WorkspaceFileFields as serde::Deserialize>::deserialize(deserializer)?;

        Self::new(fields.path, fields.bytes)
            .map_err(|error| serde::de::Error::custom(format!("invalid Project File: {error:?}")))
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for DocumentWorkspace {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct DocumentWorkspaceWire<'a> {
            root_path: &'a str,
            files: &'a [WorkspaceFile],
        }

        DocumentWorkspaceWire {
            root_path: self.root_path.get_without_slash(),
            files: &self.files,
        }
        .serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for DocumentWorkspace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct DocumentWorkspaceFields {
            root_path: String,
            files: Vec<WorkspaceFile>,
        }

        let fields = <DocumentWorkspaceFields as serde::Deserialize>::deserialize(deserializer)?;

        Self::new(fields.root_path, fields.files)
            .map_err(|error| serde::de::Error::custom(format!("invalid Typst Project: {error:?}")))
    }
}

/// A validation failure for a Typst Project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceValidationError {
    /// A Project Path is not root-relative inside the Typst Project.
    InvalidPath { path: String },

    /// More than one Project File has the same Project Path.
    DuplicatePath { path: String },

    /// The requested root Typst entrypoint does not exist in the Typst Project.
    MissingRoot { root: String },
}
