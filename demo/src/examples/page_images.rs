use crate::{SAMPLE_TYPST, download_error_summary, render_error_summary};
use diotypst::{
    DocumentWorkspace, DownloadFormat, PageImageOptions, RenderDownloadError, render_download,
    trigger_browser_download, use_typst_defaults,
};
use dioxus::prelude::*;

use crate::components::StatusLine;

/// Render one PNG Page Image per page and download them together as a Page
/// Image Archive. `DownloadFormat::PageImage` would pick a single page
/// instead; HTML artifacts are preview-only and have no download format.
#[component]
pub fn PageImagesExample() -> Element {
    let mut status = use_signal(String::new);
    let environment = use_typst_defaults().environment().clone();

    rsx! {
        button {
            class: "btn btn-primary",
            onclick: move |_| {
                let project = DocumentWorkspace::from_source(format!(
                    "{SAMPLE_TYPST}\n#pagebreak()\nRendered on a second page."
                ));
                let format = DownloadFormat::PageImageArchive {
                    options: PageImageOptions::default(),
                };

                match render_download(&project, &environment, format, "pages.zip") {
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
            "Render & download page images ZIP"
        }
        StatusLine { status }
    }
}
