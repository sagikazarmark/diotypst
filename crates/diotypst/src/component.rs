#![cfg(feature = "dioxus")]

use crate::RenderArtifact;
use crate::render_state::RenderStatus;
use crate::session::{RenderSessionOptions, TypstInput, TypstView, use_render_session};
use dioxus::prelude::*;

/// Render Typst input as an explicit high-level Dioxus view.
///
/// A thin view over a Render Session: it shows the best available Render Artifact
/// (current or stale), appends a `typst-error` block when the latest render failed, and
/// exposes the render status through a `data-render-status` attribute (`empty`,
/// `current`, `stale`, `failed`) so apps style states without prescribed markup.
/// World Preparation progress is not rendered; read it from
/// [`use_render_session`] directly when the app wants that chrome.
///
/// The component re-renders whenever `input` changes, so the caller's signal wiring is
/// the Render Policy: pass a debounced or committed value rather than raw keystrokes.
/// The optional `inputs` prop merges Typst `sys.inputs` values over the provider
/// environment for this component instance only.
#[component]
pub fn Typst(
    input: TypstInput,
    view: TypstView,
    inputs: Option<typst::foundations::Dict>,
) -> Element {
    let mut options = RenderSessionOptions::new();
    if let Some(inputs) = inputs {
        options = options.merge_inputs(inputs);
    }

    let session = use_render_session(input, view, options);
    let renderer = session.state();
    let renderer = renderer.read();
    let state = renderer.state();
    let status = match state.status() {
        RenderStatus::Empty => "empty",
        RenderStatus::Current => "current",
        RenderStatus::Stale => "stale",
        RenderStatus::Failed => "failed",
    };
    let error = state.error().map(|error| format!("{error:?}"));

    rsx! {
        div { class: "typst", "data-render-status": "{status}",
            match state.artifact() {
                Some(RenderArtifact::Html(html)) => {
                    let html = html.as_str().to_owned();

                    rsx! {
                        div { class: "typst-html", dangerous_inner_html: "{html}" }
                    }
                }
                Some(RenderArtifact::Pdf(pdf)) => {
                    let src = format!("data:application/pdf;base64,{}", base64_encode(pdf.bytes()));

                    rsx! {
                        iframe { class: "typst-pdf-frame", title: "Typst PDF", src: "{src}" }
                    }
                }
                Some(RenderArtifact::PageImages(images)) => {
                    let pages = (0..images.page_count())
                        .filter_map(|index| {
                            let page = images.page(index)?;
                            Some((
                                index + 1,
                                format!("data:image/png;base64,{}", base64_encode(page.bytes())),
                                page.width(),
                                page.height(),
                            ))
                        })
                        .collect::<Vec<_>>();

                    rsx! {
                        div { class: "typst-page-images",
                            for (page_number, src, width, height) in pages {
                                img {
                                    class: "typst-page-image",
                                    alt: "Typst page {page_number}",
                                    src: "{src}",
                                    width: "{width}",
                                    height: "{height}",
                                }
                            }
                        }
                    }
                }
                None => rsx! {},
            }
            if let Some(error) = error {
                pre { class: "typst-error", "{error}" }
            }
        }
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let value = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);

        output.push(TABLE[((value >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((value >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[((value >> 6) & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(value & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }

    output
}
