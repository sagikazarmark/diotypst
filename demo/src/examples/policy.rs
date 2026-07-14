use crate::{DENIED_PACKAGE_TEMPLATE, preparation_phase_label};
use diotypst::{RenderSessionOptions, TypstInput, TypstView, use_render_session};
use dioxus::prelude::*;

use super::PreparationPackageList;
use crate::components::{DemoPane, SourcePanel, StatusChip};

/// The demo Package Policy is a deny-all allowlist (CetZ and its dependencies,
/// plus the embedded `@demo` namespace). This project imports a package
/// outside the allowlist, so World Preparation reports it as denied before any
/// network request; the server-side proxy enforces the same policy
/// authoritatively. The Render Session exposes preparation as its own
/// dimension, so this example reads it without mounting a preview.
#[component]
pub fn PolicyExample() -> Element {
    let session = use_render_session(
        TypstInput::source(DENIED_PACKAGE_TEMPLATE),
        TypstView::Html,
        RenderSessionOptions::new(),
    );

    let preparation = session.preparation();
    let state = preparation.read();
    let phase = preparation_phase_label(state.phase());

    rsx! {
        DemoPane {
            label: "World Preparation",
            accessory: rsx! { StatusChip { label: phase } },
            SourcePanel { source: DENIED_PACKAGE_TEMPLATE }
            PreparationPackageList { preparation }
        }
    }
}
