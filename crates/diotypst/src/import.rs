//! Project Import from Dioxus file events.
//!
//! These helpers turn [`FileData`] values from a Dioxus file input event (see
//! `Event<FormData>::files()`) into Project Files and Font Set candidates, so browser, desktop,
//! and fullstack apps share one import path.

use dioxus::html::FileData;

/// A file read from a Dioxus file input event, classified for Typst Project use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedProjectFile {
    path: String,
    bytes: Vec<u8>,
    content_type: Option<String>,
    kind: ImportedFileKind,
}

impl ImportedProjectFile {
    /// Create an imported file from a normalized root-relative path and bytes.
    pub fn new(
        path: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        content_type: Option<String>,
    ) -> Self {
        let path = path.into();
        let bytes = bytes.into();
        let kind = classify_imported_file(&path, content_type.as_deref(), &bytes);

        Self {
            path,
            bytes,
            content_type,
            kind,
        }
    }

    /// Return the root-relative Project Path candidate for this file.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Return this file's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Return the content type reported by the file input, if any.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Return how this file was classified for Typst Project use.
    pub fn kind(&self) -> ImportedFileKind {
        self.kind
    }
}

/// Classification of an imported file for Typst Project use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportedFileKind {
    /// A `.typ` Typst source file; a root entrypoint candidate.
    TypstSource,

    /// A font file; a Font Set candidate rather than a Project File.
    Font,

    /// Any other file, available to Typst as an asset by Project Path.
    Asset,
}

/// Options controlling one [`import_project_files`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileImportOptions {
    max_file_bytes: Option<u64>,
    strip_container_directory: bool,
}

impl FileImportOptions {
    /// Reject files larger than the given size; the default limit is 16 MiB.
    pub fn max_file_bytes(mut self, max_file_bytes: u64) -> Self {
        self.max_file_bytes = Some(max_file_bytes);
        self
    }

    /// Accept files of any size.
    pub fn unlimited_file_bytes(mut self) -> Self {
        self.max_file_bytes = None;
        self
    }

    /// Control whether the selected container directory is stripped from multi-segment paths.
    ///
    /// Directory pickers report paths as `<picked-dir>/<nested path>`; stripping the first
    /// segment (the default) makes the picked directory the Typst Project root. Single-segment
    /// paths from plain file pickers are never stripped.
    pub fn strip_container_directory(mut self, strip: bool) -> Self {
        self.strip_container_directory = strip;
        self
    }
}

impl Default for FileImportOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: Some(16 * 1024 * 1024),
            strip_container_directory: true,
        }
    }
}

/// A Project Import failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileImportError {
    /// A file could not be read from the file input.
    Read { name: String, message: String },

    /// A file exceeds the configured size limit.
    TooLarge { name: String, size: u64, limit: u64 },
}

/// Read Dioxus file input files into classified imported files.
///
/// Sizes are checked before reading so oversized files never reach memory. The first failure
/// aborts the import; partially imported files are discarded.
pub async fn import_project_files(
    files: Vec<FileData>,
    options: FileImportOptions,
) -> Result<Vec<ImportedProjectFile>, FileImportError> {
    let mut imported = Vec::with_capacity(files.len());

    for file in files {
        if let Some(limit) = options.max_file_bytes
            && file.size() > limit
        {
            return Err(FileImportError::TooLarge {
                name: file.name(),
                size: file.size(),
                limit,
            });
        }

        let path = project_path_from_import(&file.path(), options.strip_container_directory);
        let bytes = file
            .read_bytes()
            .await
            .map_err(|error| FileImportError::Read {
                name: file.name(),
                message: error.to_string(),
            })?;

        imported.push(ImportedProjectFile::new(
            path,
            bytes.to_vec(),
            file.content_type(),
        ));
    }

    Ok(imported)
}

/// Normalize a file input path into a root-relative Project Path candidate.
///
/// On the web, `FileData::path()` carries `webkitRelativePath` for directory picks and the
/// bare file name for plain picks; on native it carries a host path whose components are
/// joined with `/`. When `strip_container` is set, the first segment of a multi-segment path
/// is dropped so the picked directory becomes the Typst Project root.
pub fn project_path_from_import(path: &std::path::Path, strip_container: bool) -> String {
    let segments = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => Some(segment.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();

    let segments = if strip_container && segments.len() > 1 {
        &segments[1..]
    } else {
        &segments[..]
    };

    segments.join("/")
}

/// Return whether a file looks like a font by extension, content type, or magic bytes.
pub fn is_font_file(path: &str, content_type: Option<&str>, bytes: &[u8]) -> bool {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);
    if extension.is_some_and(|extension| {
        ["ttf", "otf", "ttc", "otc"]
            .iter()
            .any(|font| extension.eq_ignore_ascii_case(font))
    }) {
        return true;
    }

    if content_type.is_some_and(|content_type| content_type.starts_with("font/")) {
        return true;
    }

    matches!(
        bytes.get(..4),
        Some([0x00, 0x01, 0x00, 0x00] | b"OTTO" | b"ttcf" | b"true")
    )
}

/// Split imported font files out for Font Set construction.
///
/// Returns the remaining Project Files and the font file bytes, typically passed to
/// [`FontSet::with_font_files`](libtypst::FontSet::with_font_files).
pub fn partition_imported_fonts(
    files: Vec<ImportedProjectFile>,
) -> (Vec<ImportedProjectFile>, Vec<Vec<u8>>) {
    let mut project_files = Vec::new();
    let mut fonts = Vec::new();

    for file in files {
        match file.kind {
            ImportedFileKind::Font => fonts.push(file.bytes),
            _ => project_files.push(file),
        }
    }

    (project_files, fonts)
}

fn classify_imported_file(
    path: &str,
    content_type: Option<&str>,
    bytes: &[u8],
) -> ImportedFileKind {
    if is_font_file(path, content_type, bytes) {
        return ImportedFileKind::Font;
    }

    if path
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("typ"))
    {
        return ImportedFileKind::TypstSource;
    }

    ImportedFileKind::Asset
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::html::NativeFileData;
    use std::path::PathBuf;

    /// An in-memory file for exercising the import flow without a browser.
    struct StubFile {
        path: PathBuf,
        bytes: Vec<u8>,
        content_type: Option<String>,
    }

    impl NativeFileData for StubFile {
        fn name(&self) -> String {
            self.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        }

        fn size(&self) -> u64 {
            self.bytes.len() as u64
        }

        fn last_modified(&self) -> u64 {
            0
        }

        fn path(&self) -> PathBuf {
            self.path.clone()
        }

        fn content_type(&self) -> Option<String> {
            self.content_type.clone()
        }

        fn read_bytes(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<bytes::Bytes, dioxus::dioxus_core::CapturedError>,
                    > + 'static,
            >,
        > {
            let bytes = bytes::Bytes::from(self.bytes.clone());
            Box::pin(std::future::ready(Ok(bytes)))
        }

        fn byte_stream(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn futures_util::Stream<
                        Item = Result<bytes::Bytes, dioxus::dioxus_core::CapturedError>,
                    >
                    + 'static
                    + Send,
            >,
        > {
            let bytes = bytes::Bytes::from(self.bytes.clone());
            Box::pin(futures_util::stream::once(std::future::ready(Ok(bytes))))
        }

        fn read_string(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<String, dioxus::dioxus_core::CapturedError>>
                    + 'static,
            >,
        > {
            let text = String::from_utf8_lossy(&self.bytes).into_owned();
            Box::pin(std::future::ready(Ok(text)))
        }

        fn inner(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn stub(path: &str, bytes: &[u8], content_type: Option<&str>) -> FileData {
        FileData::new(StubFile {
            path: PathBuf::from(path),
            bytes: bytes.to_vec(),
            content_type: content_type.map(str::to_owned),
        })
    }

    #[test]
    fn project_paths_strip_only_multi_segment_containers() {
        let strip = true;
        assert_eq!(
            project_path_from_import(std::path::Path::new("picked/sub/main.typ"), strip),
            "sub/main.typ"
        );
        assert_eq!(
            project_path_from_import(std::path::Path::new("main.typ"), strip),
            "main.typ"
        );
        assert_eq!(
            project_path_from_import(std::path::Path::new("picked/main.typ"), false),
            "picked/main.typ"
        );
    }

    #[test]
    fn font_detection_covers_extension_content_type_and_magic_bytes() {
        assert!(is_font_file("fonts/Custom.TTF", None, b""));
        assert!(is_font_file("custom", Some("font/woff2"), b""));
        assert!(is_font_file("custom.bin", None, b"OTTO rest-of-font"));
        assert!(is_font_file(
            "custom.bin",
            None,
            &[0x00, 0x01, 0x00, 0x00, 0xff]
        ));
        assert!(!is_font_file("image.png", Some("image/png"), b"\x89PNG"));
    }

    #[tokio::test]
    async fn import_classifies_and_partitions_files() {
        let files = vec![
            stub("picked/main.typ", b"Hello", Some("text/plain")),
            stub("picked/logo.png", b"\x89PNG", Some("image/png")),
            stub("picked/fonts/custom.otf", b"OTTO", None),
        ];

        let imported = import_project_files(files, FileImportOptions::default())
            .await
            .expect("import should succeed");

        assert_eq!(imported.len(), 3);
        assert_eq!(imported[0].path(), "main.typ");
        assert_eq!(imported[0].kind(), ImportedFileKind::TypstSource);
        assert_eq!(imported[1].kind(), ImportedFileKind::Asset);
        assert_eq!(imported[2].kind(), ImportedFileKind::Font);

        let (project_files, fonts) = partition_imported_fonts(imported);
        assert_eq!(project_files.len(), 2);
        assert_eq!(fonts, vec![b"OTTO".to_vec()]);
    }

    #[tokio::test]
    async fn import_rejects_oversized_files_before_reading() {
        let files = vec![stub("big.bin", &[0u8; 32], None)];

        let error = import_project_files(files, FileImportOptions::default().max_file_bytes(16))
            .await
            .expect_err("oversized file should be rejected");

        assert_eq!(
            error,
            FileImportError::TooLarge {
                name: "big.bin".to_owned(),
                size: 32,
                limit: 16,
            }
        );
    }
}
