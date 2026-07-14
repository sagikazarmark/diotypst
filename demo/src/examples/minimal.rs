use diotypst::{RenderSessionOptions, TypstInput, TypstView, use_render_session};
use dioxus::prelude::*;

use super::TypstPreview;

const SOURCE: &str = r#"#set page(width: 120mm, height: auto, margin: 10mm)

= Dioxus + Typst

The smallest complete flow: a Typst Project rendered to
semantic HTML by one Render Session.
"#;

/// The smallest useful render: one Render Session over a Typst Project from
/// one source string. Rendering is deterministic, so the session is
/// declarative: it renders on mount and again whenever its input changes —
/// there is nothing to trigger for a constant source.
#[component]
pub fn MinimalExample() -> Element {
    let session = use_render_session(
        TypstInput::source(SOURCE),
        TypstView::Html,
        RenderSessionOptions::new(),
    );

    rsx! {
        TypstPreview { render: session.state() }
    }
}
