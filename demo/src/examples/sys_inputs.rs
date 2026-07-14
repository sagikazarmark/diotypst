use std::time::Duration;

use diotypst::{RenderSessionOptions, TypstInput, TypstView, use_render_session};
use dioxus::prelude::*;
use dioxus_sdk_time::use_debounce;

use super::TypstPreview;
use crate::components::{DemoPane, DemoSurface};

const SOURCE: &str = r#"#set page(width: 120mm, height: auto, margin: 10mm)

Hello, #sys.inputs.at("name", default: "world")!
"#;

/// System Inputs are explicit Typst values installed into the Render
/// Environment and visible to document code through `sys.inputs`. The session
/// re-renders whenever the environment changes, so the keystroke stream is
/// debounced before it reaches the session's input value.
#[component]
pub fn SysInputsExample() -> Element {
    let mut name = use_signal(String::new);
    let mut session_name = use_signal(String::new);

    let mut debounce = use_debounce(Duration::from_millis(300), move |value| {
        session_name.set(value);
    });

    let mut options = RenderSessionOptions::new();
    if !session_name.read().trim().is_empty() {
        options = options.input("name", session_name.read().trim());
    }
    let session = use_render_session(TypstInput::source(SOURCE), TypstView::Html, options);

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane { label: "Live",
                    label { class: "block space-y-1",
                        span { class: "text-sm font-medium", "sys.inputs name" }
                        input {
                            class: "input input-bordered w-full",
                            r#type: "text",
                            placeholder: "world",
                            value: "{name}",
                            oninput: move |event| {
                                name.set(event.value());
                                debounce.action(event.value());
                            },
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
