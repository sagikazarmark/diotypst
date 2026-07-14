//! Presentation used specifically by the Typst examples in this application.

use diotypst::{HeadlessRender, RenderArtifact, WorldPreparationState};
use dioxus::prelude::*;

use crate::components::{DemoPane, StatusChip};
use crate::{html_body_fragment, package_status_label, render_error_summary, render_status_label};

/// Per-package progress emitted by World Preparation.
#[component]
pub fn PreparationPackageList(preparation: Signal<WorldPreparationState>) -> Element {
    let packages = preparation
        .read()
        .packages()
        .iter()
        .map(|entry| {
            (
                entry.spec().to_string(),
                package_status_label(entry.status()),
                entry.message().map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();

    rsx! {
        ul { class: "mt-2 space-y-1 font-mono text-xs",
            for (spec , status , message) in packages {
                li { class: "flex flex-wrap items-center gap-2",
                    code { "{spec}" }
                    span { class: "text-base-content/55", "{status}" }
                    if let Some(message) = message {
                        span { class: "text-error", "{message}" }
                    }
                }
            }
        }
    }
}

/// Demo chrome around a Typst render, including status and diagnostics.
#[component]
pub fn TypstPreview(
    render: Signal<HeadlessRender>,
    #[props(into, default = String::from("Preview"))] label: String,
) -> Element {
    let renderer = render.read();
    let state = renderer.state();
    let status = render_status_label(state.status());
    let html = match state.artifact() {
        Some(RenderArtifact::Html(html)) => Some(html_body_fragment(html.as_str()).to_owned()),
        _ => None,
    };
    let error = state.error().map(render_error_summary);

    rsx! {
        DemoPane {
            label,
            accessory: rsx! { StatusChip { label: status } },
            if let Some(html) = html {
                div {
                    class: "typst-preview rounded-xl border border-base-300 bg-base-100 p-4",
                    dangerous_inner_html: "{html}",
                }
            } else {
                div { class: "rounded-xl border border-dashed border-base-300 bg-base-100 p-6 text-sm text-base-content/55",
                    "No artifact yet. Render to compile the Typst Project into semantic HTML."
                }
            }
            if let Some(error) = error {
                pre { class: "mt-3 overflow-x-auto rounded-xl bg-error/10 p-3 font-mono text-xs text-error",
                    "{error}"
                }
            }
        }
    }
}
