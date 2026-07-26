/// A selectable Render Artifact format.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderFormat {
    /// PDF output.
    Pdf,

    /// PNG Page Image output.
    PageImages(PageImageOptions),

    /// Semantic HTML output.
    Html,
}

/// A Render Artifact selected at runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderArtifact {
    /// PDF output.
    Pdf(PdfArtifact),

    /// PNG Page Image output.
    PageImages(PageImagesArtifact),

    /// Semantic HTML output.
    Html(HtmlArtifact),
}

/// A rendered PDF.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PdfArtifact {
    pub(crate) bytes: Vec<u8>,
}

impl PdfArtifact {
    /// Return the PDF bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// A rendered HTML artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HtmlArtifact {
    pub(crate) html: String,
}

impl HtmlArtifact {
    /// Return the HTML text.
    pub fn as_str(&self) -> &str {
        &self.html
    }
}

/// Requested fidelity settings for Page Image output.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageImageOptions {
    pixel_per_pt: f32,
}

impl PageImageOptions {
    /// Create Page Image Options with the given pixel-per-point scale.
    pub fn new(pixel_per_pt: f32) -> Self {
        Self { pixel_per_pt }
    }

    /// Return the configured pixel-per-point scale.
    pub fn pixel_per_pt(&self) -> f32 {
        self.pixel_per_pt
    }
}

impl Default for PageImageOptions {
    fn default() -> Self {
        Self { pixel_per_pt: 2.0 }
    }
}

/// Rendered PNG Page Images.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageImagesArtifact {
    pub(crate) pages: Vec<PageImage>,
}

impl PageImagesArtifact {
    /// Return the number of rendered Page Images.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Return the rendered Page Images in page order.
    pub fn pages(&self) -> &[PageImage] {
        &self.pages
    }

    /// Return a Page Image by zero-based page index.
    pub fn page(&self, index: usize) -> Option<&PageImage> {
        self.pages.get(index)
    }
}

/// A PNG image for one rendered Typst page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageImage {
    pub(crate) bytes: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl PageImage {
    /// Return the PNG bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Return the rendered image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Return the rendered image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }
}
