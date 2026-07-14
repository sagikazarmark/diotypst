use std::time::Duration;

use crate::{JSON_DATA_TEMPLATE, SAMPLE_JSON_DATA};
use diotypst::{
    RenderSessionOptions, TypstInput, TypstView, use_render_session, use_typst_defaults,
};
use dioxus::prelude::*;
use dioxus_code_editor::{CodeEditor, Language};
use dioxus_sdk_time::use_debounce;

use super::TypstPreview;
use crate::components::{DemoPane, DemoSurface, SourcePanel, StatusChip, snippet_theme};

/// System Inputs are strings, so structured data travels as JSON: the app
/// validates it with serde before it reaches the Render Environment, and the
/// document decodes it with `json(bytes(sys.inputs.at("data")))`. The editor
/// keystrokes are debounced before the JSON reaches the session's environment.
/// Invalid JSON surfaces twice: as the app-side parse error below the editor,
/// and as a Stale Artifact with Typst's own diagnostics in the preview.
#[component]
pub fn JsonInputsExample() -> Element {
    let defaults = use_typst_defaults();
    let mut data = use_signal(|| SAMPLE_JSON_DATA.to_owned());
    let mut session_data = use_signal(|| SAMPLE_JSON_DATA.to_owned());

    let mut debounce = use_debounce(Duration::from_millis(300), move |value| {
        session_data.set(value);
    });

    let parse_error = serde_json::from_str::<serde_json::Value>(&data.read())
        .err()
        .map(|error| error.to_string());

    let environment = defaults
        .environment()
        .to_builder()
        .input("data", session_data.read().as_str())
        .build()
        .expect("render environment with a JSON input should be valid");
    let session = use_render_session(
        TypstInput::source(JSON_DATA_TEMPLATE),
        TypstView::Html,
        RenderSessionOptions::new().environment(environment),
    );

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane {
                    label: "sys.inputs data (JSON)",
                    accessory: rsx! {
                        StatusChip { label: if parse_error.is_some() { "invalid JSON" } else { "valid JSON" } }
                    },
                    SourcePanel { source: JSON_DATA_TEMPLATE }
                    CodeEditor {
                        class: "mt-3 max-h-64 overflow-auto rounded-xl border border-base-300 bg-base-100 font-mono text-sm",
                        value: data.read().clone(),
                        language: Language::Json,
                        theme: snippet_theme(),
                        spellcheck: false,
                        aria_label: "JSON data editor",
                        oninput: move |value: String| {
                            data.set(value.clone());
                            debounce.action(value);
                        },
                    }
                    if let Some(error) = parse_error {
                        p { class: "mt-2 rounded-lg bg-error/10 px-3 py-2 font-mono text-xs text-error",
                            "{error}"
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
