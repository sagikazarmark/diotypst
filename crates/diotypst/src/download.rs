use crate::{
    DocumentWorkspace, PageImage, PageImageOptions, PageImagesArtifact, PdfArtifact,
    RenderArtifact, RenderEnvironment, RenderError, RenderFormat, RenderState, render_artifact,
};

/// A downloadable output format for a Download Action.
///
/// Unlike [`RenderFormat`], this describes bytes a user saves: HTML is not downloadable,
/// and Page Images can be requested as one page or as a Page Image Archive.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DownloadFormat {
    /// PDF download output.
    Pdf,

    /// One PNG Page Image download output.
    PageImage {
        /// Zero-based page index to download.
        page_index: usize,

        /// Page Image render options.
        options: PageImageOptions,
    },

    /// ZIP archive containing one PNG Page Image per rendered Typst page.
    PageImageArchive {
        /// Page Image render options.
        options: PageImageOptions,
    },
}

impl DownloadFormat {
    fn render_format(self) -> RenderFormat {
        match self {
            Self::Pdf => RenderFormat::Pdf,
            Self::PageImage { options, .. } | Self::PageImageArchive { options } => {
                RenderFormat::PageImages(options)
            }
        }
    }
}

/// A Download Action failure while rendering on demand.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderDownloadError {
    /// Typst rendering failed.
    Render(RenderError),

    /// The rendered artifact could not be prepared as the requested download.
    Download(DownloadError),
}

impl From<RenderError> for RenderDownloadError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

impl From<DownloadError> for RenderDownloadError {
    fn from(error: DownloadError) -> Self {
        Self::Download(error)
    }
}

/// Render a Typst Project on demand and prepare the requested Download File.
///
/// This is the render-on-demand path of a Download Action, shared by every Download
/// Backend: client-side rendering and the Server Render Route produce identical bytes
/// for the same inputs. Rendering dispatches through [`render_artifact`]; packaging
/// (page selection, Page Image Archive assembly) happens here.
pub fn render_download(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
    format: DownloadFormat,
    filename: impl Into<String>,
) -> Result<DownloadFile, RenderDownloadError> {
    let artifact = render_artifact(workspace, environment, format.render_format())?;

    let file = match (format, artifact) {
        (DownloadFormat::Pdf, RenderArtifact::Pdf(pdf)) => DownloadFile::from_pdf(filename, &pdf),
        (DownloadFormat::PageImage { page_index, .. }, RenderArtifact::PageImages(page_images)) => {
            let page_image = page_images
                .page(page_index)
                .ok_or(DownloadError::Unavailable)?;

            DownloadFile::from_page_image(filename, page_image)
        }
        (DownloadFormat::PageImageArchive { .. }, RenderArtifact::PageImages(page_images)) => {
            DownloadFile::from_page_images_archive(filename, &page_images)
        }
        _ => unreachable!("the rendered artifact kind follows the download format"),
    };

    Ok(file)
}

/// A render artifact prepared for a user-triggered download.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadFile {
    filename: String,
    media_type: &'static str,
    bytes: Vec<u8>,
}

impl DownloadFile {
    /// Create a Download File from a runtime-selected Render Artifact.
    pub fn from_render_artifact(
        filename: impl Into<String>,
        artifact: &RenderArtifact,
    ) -> Result<Self, DownloadError> {
        match artifact {
            RenderArtifact::Pdf(pdf) => Ok(Self::from_pdf(filename, pdf)),
            RenderArtifact::PageImages(page_images) => {
                Ok(Self::from_page_images_archive(filename, page_images))
            }
            RenderArtifact::Html(_) => Err(DownloadError::UnsupportedArtifact),
        }
    }

    /// Create a Download File from the current or stale runtime-selected Render Artifact.
    pub fn from_render_artifact_state(
        filename: impl Into<String>,
        state: &RenderState<RenderArtifact>,
    ) -> Result<Self, DownloadError> {
        state
            .artifact()
            .map(|artifact| Self::from_render_artifact(filename, artifact))
            .unwrap_or(Err(DownloadError::Unavailable))
    }

    /// Create a Download File from a PDF artifact.
    pub fn from_pdf(filename: impl Into<String>, pdf: &PdfArtifact) -> Self {
        Self {
            filename: filename.into(),
            media_type: "application/pdf",
            bytes: pdf.bytes().to_vec(),
        }
    }

    /// Create a Download File from a PNG Page Image.
    pub fn from_page_image(filename: impl Into<String>, page_image: &PageImage) -> Self {
        Self {
            filename: filename.into(),
            media_type: "image/png",
            bytes: page_image.bytes().to_vec(),
        }
    }

    /// Create a Download File from multiple PNG Page Images as a ZIP archive.
    ///
    /// Page Images are supplied as a [`PageImagesArtifact`], so callers cannot construct invalid
    /// archive entries through the public interface. If a renderer ever yields no pages, the
    /// private archive writer still emits a valid empty ZIP file.
    ///
    /// The current archive writer emits ZIP32 store-only archives. It refuses to silently wrap
    /// ZIP32-sized entry counts, offsets, names, or byte ranges; add ZIP64 support or a
    /// target-compatible archive dependency before those limits are expected in normal use.
    pub fn from_page_images_archive(
        filename: impl Into<String>,
        page_images: &PageImagesArtifact,
    ) -> Self {
        let entries = page_images
            .pages()
            .iter()
            .enumerate()
            .map(|(index, page_image)| (format!("page-{}.png", index + 1), page_image.bytes()));
        let bytes = zip_store_entries(entries);

        Self {
            filename: filename.into(),
            media_type: "application/zip",
            bytes,
        }
    }

    /// Create a `.typk` Download File from a Project Pack.
    ///
    /// The media type stays `application/octet-stream` rather than
    /// `application/zip` so browsers with archive auto-expansion keep the
    /// pack as one re-importable file.
    #[cfg(feature = "pack")]
    pub fn from_project_pack(
        filename: impl Into<String>,
        pack: &crate::ProjectPack,
    ) -> Result<Self, crate::ProjectPackError> {
        Ok(Self {
            filename: filename.into(),
            media_type: "application/octet-stream",
            bytes: pack.to_bytes()?,
        })
    }

    /// Return the suggested download filename.
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Return the media type for the download response or browser Blob.
    pub fn media_type(&self) -> &str {
        self.media_type
    }

    /// Return the downloadable bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A download preparation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DownloadError {
    /// No suitable Render Artifact is available for the requested download.
    Unavailable,

    /// The Render Artifact exists but is not a supported download format.
    UnsupportedArtifact,
}

/// Trigger a browser Download Action for a prepared Download File.
///
/// This helper is only available in Wasm browser builds. It creates a Blob object URL from the
/// file bytes, clicks a temporary download link, removes that link, and revokes the object URL.
#[cfg(target_arch = "wasm32")]
pub fn trigger_browser_download(file: &DownloadFile) -> Result<(), BrowserDownloadError> {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().ok_or(BrowserDownloadError::WindowUnavailable)?;
    let document = window
        .document()
        .ok_or(BrowserDownloadError::DocumentUnavailable)?;
    let body = document
        .body()
        .ok_or(BrowserDownloadError::DocumentBodyUnavailable)?;

    let bytes = js_sys::Uint8Array::from(file.bytes());
    let parts = js_sys::Array::new();
    parts.push(&bytes);

    let options = web_sys::BlobPropertyBag::new();
    options.set_type(file.media_type());
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(parts.as_ref(), &options)
        .map_err(|_| BrowserDownloadError::BrowserOperationFailed)?;
    let object_url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| BrowserDownloadError::BrowserOperationFailed)?;

    let click_result = (|| {
        let anchor = document
            .create_element("a")
            .map_err(|_| BrowserDownloadError::BrowserOperationFailed)?
            .dyn_into::<web_sys::HtmlAnchorElement>()
            .map_err(|_| BrowserDownloadError::BrowserOperationFailed)?;
        anchor.set_href(&object_url);
        anchor.set_download(file.filename());
        anchor
            .set_attribute("style", "display: none")
            .map_err(|_| BrowserDownloadError::BrowserOperationFailed)?;

        body.append_child(&anchor)
            .map_err(|_| BrowserDownloadError::BrowserOperationFailed)?;
        anchor.click();
        body.remove_child(&anchor)
            .map_err(|_| BrowserDownloadError::BrowserOperationFailed)?;

        Ok(())
    })();

    let revoke_result = web_sys::Url::revoke_object_url(&object_url)
        .map_err(|_| BrowserDownloadError::BrowserOperationFailed);

    click_result.and(revoke_result)
}

/// A browser Download Action failure.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserDownloadError {
    /// No browser window is available.
    WindowUnavailable,

    /// No document is available in the browser window.
    DocumentUnavailable,

    /// No document body is available to host the temporary download link.
    DocumentBodyUnavailable,

    /// A browser DOM, Blob, or object URL operation failed.
    BrowserOperationFailed,
}

struct ZipEntryMetadata {
    name: String,
    crc32: u32,
    size: u32,
    local_header_offset: u32,
}

fn zip_store_entries<'a>(entries: impl IntoIterator<Item = (String, &'a [u8])>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut metadata = Vec::new();

    for (name, bytes) in entries {
        let crc32 = crc32(bytes);
        let size = zip_u32(bytes.len(), "Page Image Archive entry size");
        let local_header_offset = zip_u32(output.len(), "Page Image Archive local header offset");

        write_zip_local_file_header(&mut output, &name, crc32, size);
        output.extend_from_slice(bytes);
        metadata.push(ZipEntryMetadata {
            name,
            crc32,
            size,
            local_header_offset,
        });
    }

    let central_directory_start = output.len();
    let central_directory_offset = zip_u32(
        central_directory_start,
        "Page Image Archive central directory offset",
    );
    for entry in &metadata {
        write_zip_central_directory_header(&mut output, entry);
    }
    let central_directory_size = zip_u32(
        output.len() - central_directory_start,
        "Page Image Archive central directory size",
    );
    write_zip_end_of_central_directory(
        &mut output,
        zip_u16(metadata.len(), "Page Image Archive entry count"),
        central_directory_size,
        central_directory_offset,
    );

    output
}

fn write_zip_local_file_header(output: &mut Vec<u8>, name: &str, crc32: u32, size: u32) {
    let name_length = zip_u16(name.len(), "Page Image Archive entry name length");

    push_u32_le(output, 0x0403_4b50);
    push_u16_le(output, 20);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u32_le(output, crc32);
    push_u32_le(output, size);
    push_u32_le(output, size);
    push_u16_le(output, name_length);
    push_u16_le(output, 0);
    output.extend_from_slice(name.as_bytes());
}

fn write_zip_central_directory_header(output: &mut Vec<u8>, entry: &ZipEntryMetadata) {
    let name_length = zip_u16(entry.name.len(), "Page Image Archive entry name length");

    push_u32_le(output, 0x0201_4b50);
    push_u16_le(output, 20);
    push_u16_le(output, 20);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u32_le(output, entry.crc32);
    push_u32_le(output, entry.size);
    push_u32_le(output, entry.size);
    push_u16_le(output, name_length);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u32_le(output, 0);
    push_u32_le(output, entry.local_header_offset);
    output.extend_from_slice(entry.name.as_bytes());
}

fn write_zip_end_of_central_directory(
    output: &mut Vec<u8>,
    entry_count: u16,
    central_directory_size: u32,
    central_directory_offset: u32,
) {
    push_u32_le(output, 0x0605_4b50);
    push_u16_le(output, 0);
    push_u16_le(output, 0);
    push_u16_le(output, entry_count);
    push_u16_le(output, entry_count);
    push_u32_le(output, central_directory_size);
    push_u32_le(output, central_directory_offset);
    push_u16_le(output, 0);
}

fn zip_u16(value: usize, context: &str) -> u16 {
    u16::try_from(value).unwrap_or_else(|_| panic!("{context} exceeds ZIP32 limits"))
}

fn zip_u32(value: usize, context: &str) -> u32 {
    u32::try_from(value).unwrap_or_else(|_| panic!("{context} exceeds ZIP32 limits"))
}

fn push_u16_le(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;

    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb_88320 & mask);
        }
    }

    !crc
}
