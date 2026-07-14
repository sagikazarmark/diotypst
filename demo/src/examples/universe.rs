use crate::{CETZ_TEMPLATE, preparation_phase_label};
use diotypst::{
    PackagePreparationStatus, RenderSessionOptions, TypstInput, TypstView, WorldPreparationPhase,
    use_render_session,
};
use dioxus::prelude::*;

use super::{PreparationPackageList, TypstPreview};
use crate::components::{DemoPane, DemoSurface, StatusChip};

/// One Render Session owns the whole flow: it renders immediately (degrading
/// to missing-package diagnostics), resolves the packages the Typst Project
/// imports through the Package Source configured on `TypstProvider` (here:
/// browser `fetch` against the same-origin package proxy), and re-renders as
/// soon as the prepared Render Environment carries the downloaded Package
/// Bundles. The preparation dimension reports per-package progress.
#[component]
pub fn UniverseExample() -> Element {
    let session = use_render_session(
        TypstInput::source(CETZ_TEMPLATE),
        TypstView::Html,
        RenderSessionOptions::new(),
    );

    let preparation = session.preparation();
    let state = preparation.read();
    let phase = preparation_phase_label(state.phase());
    let failed = state.phase() == WorldPreparationPhase::Failed
        || state.packages().iter().any(|entry| {
            matches!(
                entry.status(),
                PackagePreparationStatus::Denied | PackagePreparationStatus::Failed
            )
        });

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane {
                    label: "World Preparation",
                    accessory: rsx! { StatusChip { label: phase } },
                    PreparationPackageList { preparation }
                    if failed {
                        button {
                            class: "btn btn-sm mt-2",
                            onclick: move |_| {
                                let mut session = session;
                                session.restart_preparation();
                            },
                            "Retry package resolution"
                        }
                    }
                }
            },
            secondary: rsx! {
                TypstPreview { render: session.state() }
            },
        }
    }
}
