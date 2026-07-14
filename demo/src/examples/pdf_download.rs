use crate::{SAMPLE_TYPST, download_error_summary, render_error_summary};
use diotypst::{
    DocumentWorkspace, DownloadFormat, RenderDownloadError, render_download,
    trigger_browser_download, use_typst_defaults,
};
use dioxus::prelude::*;

use crate::components::StatusLine;

/// Render a Typst Project to PDF entirely in the browser and hand the bytes to
/// the browser's download flow. `render_download` is the same Download Action
/// the Server Render Route uses, so client and server downloads produce
/// identical bytes for the same inputs.
#[component]
pub fn PdfDownloadExample() -> Element {
    let mut status = use_signal(String::new);
    let environment = use_typst_defaults().environment().clone();

    rsx! {
        button {
            class: "btn btn-primary",
            onclick: move |_| {
                let project = DocumentWorkspace::from_source(SAMPLE_TYPST);
                match render_download(&project, &environment, DownloadFormat::Pdf, "document.pdf") {
                    Ok(file) => match trigger_browser_download(&file) {
                        Ok(()) => status.set(format!("Downloaded {}.", file.filename())),
                        Err(error) => status.set(format!("Browser download failed: {error:?}")),
                    },
                    Err(RenderDownloadError::Render(error)) => {
                        status.set(render_error_summary(&error))
                    }
                    Err(RenderDownloadError::Download(error)) => {
                        status.set(download_error_summary(&error).to_owned())
                    }
                }
            },
            "Render & download PDF"
        }
        StatusLine { status }
    }
}
