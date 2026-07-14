use crate::SAMPLE_TYPST;
use dioxus::prelude::*;

/// Plain HTML forms posting Typst source to the native server's render
/// routes: `/typst/demo-download` renders through the same explicit Project
/// World as the browser examples, but on the server, and answers with a
/// downloadable PDF or Page Image Archive. No JavaScript is involved, so this
/// works with the fullstack server flavor only (the Cloudflare Worker
/// deployment serves the package proxy, not the render routes).
#[component]
pub fn ServerDownloadExample() -> Element {
    rsx! {
        form { class: "space-y-3", method: "post", action: "/typst/demo-download",
            input { r#type: "hidden", name: "format", value: "pdf" }
            input { r#type: "hidden", name: "filename", value: "server-document.pdf" }
            textarea {
                class: "textarea textarea-bordered w-full font-mono text-xs",
                rows: 8,
                name: "source",
                spellcheck: false,
                "{SAMPLE_TYPST}"
            }
            button { class: "btn btn-primary", r#type: "submit", "Download PDF from the server" }
        }
        form { class: "mt-4 space-y-3", method: "post", action: "/typst/demo-download",
            input { r#type: "hidden", name: "format", value: "page-image-archive" }
            input { r#type: "hidden", name: "filename", value: "server-pages.zip" }
            input {
                r#type: "hidden",
                name: "source",
                value: "{SAMPLE_TYPST}\n#pagebreak()\nRendered on a second page.",
            }
            button { class: "btn", r#type: "submit", "Download page images ZIP from the server" }
        }
    }
}
