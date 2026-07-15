use crate::observe::{file_id_package, file_id_path};
use typst::diag::SourceDiagnostic;
use typst::syntax::VirtualPath;
use typst::{World, WorldExt};

/// A Typst diagnostic surfaced through the render interface.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderDiagnostic {
    message: String,
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_workspace_path"))]
    workspace_path: Option<VirtualPath>,
    source_identity: Option<RenderSourceIdentity>,
    source_range: Option<RenderSourceRange>,
}

/// Serialize a workspace path as its root-relative text.
#[cfg(feature = "serde")]
fn serialize_workspace_path<S>(path: &Option<VirtualPath>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match path {
        Some(path) => serializer.serialize_some(path.get_without_slash()),
        None => serializer.serialize_none(),
    }
}

impl RenderDiagnostic {
    /// Return the diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Return the Project Path this diagnostic points to, when available.
    pub fn workspace_path(&self) -> Option<&VirtualPath> {
        self.workspace_path.as_ref()
    }

    /// Return the Typst source identity this diagnostic points to, when available.
    pub fn source_identity(&self) -> Option<&RenderSourceIdentity> {
        self.source_identity.as_ref()
    }

    /// Return the source range this diagnostic points to, when available.
    pub fn source_range(&self) -> Option<RenderSourceRange> {
        self.source_range
    }
}

/// A Typst source identity for a render diagnostic.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderSourceIdentity {
    package: Option<String>,
    path: String,
}

impl RenderSourceIdentity {
    /// Return the package spec this source belongs to, when it is a package source.
    pub fn package(&self) -> Option<&str> {
        self.package.as_deref()
    }

    /// Return the source path within its workspace or package.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// A zero-based source range for a render diagnostic.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderSourceRange {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

impl RenderSourceRange {
    /// Return the zero-based line where the diagnostic range starts.
    pub fn start_line(&self) -> usize {
        self.start_line
    }

    /// Return the zero-based column where the diagnostic range starts.
    pub fn start_column(&self) -> usize {
        self.start_column
    }

    /// Return the zero-based line where the diagnostic range ends.
    pub fn end_line(&self) -> usize {
        self.end_line
    }

    /// Return the zero-based column where the diagnostic range ends.
    pub fn end_column(&self) -> usize {
        self.end_column
    }
}

// Only the render backends convert Typst diagnostics; a build without any Render
// Capability never compiles documents.
#[cfg_attr(
    not(any(feature = "pdf", feature = "page-images", feature = "html")),
    allow(dead_code)
)]
pub(crate) fn to_render_diagnostics(
    world: &dyn World,
    diagnostics: impl IntoIterator<Item = SourceDiagnostic>,
) -> Vec<RenderDiagnostic> {
    diagnostics
        .into_iter()
        .map(|diagnostic| RenderDiagnostic {
            message: diagnostic.message.to_string(),
            workspace_path: diagnostic_workspace_path(&diagnostic),
            source_identity: diagnostic_source_identity(&diagnostic),
            source_range: diagnostic_source_range(world, &diagnostic),
        })
        .collect()
}

fn diagnostic_workspace_path(diagnostic: &SourceDiagnostic) -> Option<VirtualPath> {
    let id = diagnostic.span.id()?;
    if file_id_package(id).is_some() {
        return None;
    }

    Some(id.vpath().clone())
}

fn diagnostic_source_identity(diagnostic: &SourceDiagnostic) -> Option<RenderSourceIdentity> {
    let id = diagnostic.span.id()?;

    Some(RenderSourceIdentity {
        package: file_id_package(id).map(|package| package.to_string()),
        path: file_id_path(id),
    })
}

fn diagnostic_source_range(
    world: &dyn World,
    diagnostic: &SourceDiagnostic,
) -> Option<RenderSourceRange> {
    let span = diagnostic.span;
    let id = span.id()?;
    let byte_range = world.range(span)?;
    let source = world.source(id).ok()?;
    let (start_line, start_column) = source.lines().byte_to_line_column(byte_range.start)?;
    let (end_line, end_column) = source.lines().byte_to_line_column(byte_range.end)?;

    Some(RenderSourceRange {
        start_line,
        start_column,
        end_line,
        end_column,
    })
}
