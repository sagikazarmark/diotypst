use std::time::Duration;

use diotypst::{RenderSessionOptions, TypstInput, TypstView, use_render_session};
use dioxus::prelude::*;
use dioxus_code_editor::{CodeEditor, Language};
use dioxus_sdk_time::use_debounce;

use super::TypstPreview;
use crate::SAMPLE_TYPST;
use crate::components::{DemoPane, DemoSurface, snippet_theme};

/// The Render Session is declarative: it renders whatever source signal it is
/// given, whenever that signal changes. The Render Policy is therefore plain
/// signal wiring with two signals: the editor shows the *live* text, while the
/// session reads a separate signal that live-preview mode sets through a 400 ms
/// debounce and explicit mode sets only on Render now. Introduce an error
/// (delete a closing bracket) to see the stale artifact: the last good render
/// stays visible while diagnostics point at the broken source.
#[component]
pub fn EditorExample() -> Element {
    let mut editor = use_signal(|| SAMPLE_TYPST.to_owned());
    let mut session_source = use_signal(|| SAMPLE_TYPST.to_owned());
    let mut live = use_signal(|| false);

    let mut debounce = use_debounce(Duration::from_millis(400), move |text| {
        session_source.set(text);
    });

    let session = use_render_session(
        TypstInput::source(session_source.read().clone()),
        TypstView::Html,
        RenderSessionOptions::new(),
    );

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane { label: "Editor",
                    CodeEditor {
                        class: "max-h-96 overflow-auto rounded-xl border border-base-300 bg-base-100 font-mono text-sm",
                        value: editor.read().clone(),
                        language: Language::Typst,
                        theme: snippet_theme(),
                        spellcheck: false,
                        aria_label: "Typst source editor",
                        oninput: move |value: String| {
                            editor.set(value.clone());
                            if live() {
                                debounce.action(value);
                            }
                        },
                    }
                    div { class: "mt-3 flex flex-wrap items-center gap-2",
                        div { class: "join",
                            button {
                                class: if live() { "btn btn-sm join-item" } else { "btn btn-sm join-item btn-active" },
                                onclick: move |_| {
                                    live.set(false);
                                    debounce.cancel();
                                },
                                "Explicit"
                            }
                            button {
                                class: if live() { "btn btn-sm join-item btn-active" } else { "btn btn-sm join-item" },
                                onclick: move |_| {
                                    live.set(true);
                                    session_source.set(editor.peek().clone());
                                },
                                "Live preview"
                            }
                        }
                        button {
                            class: "btn btn-sm btn-primary",
                            onclick: move |_| session_source.set(editor.peek().clone()),
                            "Render now"
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
