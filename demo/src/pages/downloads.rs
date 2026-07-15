use dioxus::prelude::*;
use dioxus_code::{Code, code};

use crate::components::{ExampleSection, InlineCode, PageHeader, snippet_theme};
use crate::examples::page_images::PageImagesExample;
use crate::examples::pdf_download::PdfDownloadExample;

#[component]
pub fn PdfDownload() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Downloads",
            title: "PDF download",
            intro: "Rendering and download preparation both happen in the browser: no server round-trip, no upload of the document source.",
        }
        ExampleSection {
            title: "render_download",
            intro: rsx! {
                InlineCode { "render_download" }
                " renders on demand and prepares the downloadable bytes in one call: the same Download Action the Server Render Route uses, so client and server downloads produce identical bytes. "
                InlineCode { "trigger_browser_download" }
                " hands them to the browser; "
                InlineCode { "DownloadFile::from_render_artifact_state" }
                " remains for downloading a session's current or stale artifact without re-rendering."
            },
            demo: rsx! { PdfDownloadExample {} },
            code: rsx! {
                Code { src: code!("src/examples/pdf_download.rs"), theme: snippet_theme() }
            },
        }
    }
}

#[component]
pub fn PageImagesDownload() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Downloads",
            title: "Page images",
            intro: "The Page Images format renders one PNG per page; the download packs them into a ZIP archive. HTML artifacts are preview-only and refuse to download.",
        }
        ExampleSection {
            title: "DownloadFormat::PageImageArchive",
            intro: rsx! {
                "The archive is assembled client-side from the rendered Page Images; "
                InlineCode { "DownloadFormat::PageImage" }
                " would pick a single page instead. HTML has no download format; asking "
                InlineCode { "DownloadFile" }
                " for an HTML artifact yields "
                InlineCode { "DownloadError::UnsupportedArtifact" }
                "."
            },
            demo: rsx! { PageImagesExample {} },
            code: rsx! {
                Code { src: code!("src/examples/page_images.rs"), theme: snippet_theme() }
            },
        }
    }
}
