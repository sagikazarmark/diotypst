use dioxus::prelude::*;
use dioxus_code::{Code, code};

use crate::components::{
    DocsCallout, ExampleSection, ExternalAction, InlineCode, PageHeader, snippet_theme,
};
use crate::examples::server_download::ServerDownloadExample;

#[component]
pub fn ServerRendering() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Server",
            title: "Server rendering",
            intro: "The server feature mounts Axum routes that render Typst Projects server-side through the same explicit Project World and answer with downloadable artifacts.",
        }
        ExampleSection {
            title: "server_render_download_router",
            intro: rsx! {
                "Plain HTML forms post the source to the native server; no JavaScript is involved. Start the fullstack client/server command from the demo README or use "
                InlineCode { "dagger call service up" }
                ". The Cloudflare Worker deployment serves the package proxy, not the render routes, so the buttons 404 there."
            },
            demo: rsx! { ServerDownloadExample {} },
            code: rsx! {
                Code { src: code!("src/examples/server_download.rs"), theme: snippet_theme() }
            },
        }
        DocsCallout {
            title: "The Server Render Route",
            action: Some(ExternalAction::new(
                "diotypst README",
                "https://github.com/sagikazarmark/diotypst/tree/main/crates/diotypst#dioxus-and-server-flows",
            )),
            "The route accepts a ServerRenderRequest (a Typst Project, a Render Environment, format, filename) and returns PDF, Page Image, or Page Image Archive responses; deserialization validates the project and environment before rendering. HTML artifacts are rejected as unsupported downloads. The demo server also mounts the package proxy with the shared allowlist."
        }
    }
}
