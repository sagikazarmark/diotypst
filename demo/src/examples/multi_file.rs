use diotypst::{DocumentWorkspace, RenderFormat, use_typst_defaults, use_typst_render};
use dioxus::prelude::*;

use super::TypstPreview;
use crate::components::{DemoPane, DemoSurface};

/// A Typst Project is one Root Entrypoint plus explicit Project Files
/// addressed by root-relative Project Paths. Includes resolve against those
/// files only: rendering never reads the host filesystem.
#[component]
pub fn MultiFileExample() -> Element {
    let mut renderer = use_typst_render();
    let environment = use_typst_defaults().environment().clone();

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane { label: "Live",
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let project = DocumentWorkspace::builder("main.typ")
                                .source_file(
                                    "main.typ",
                                    "#set page(width: 120mm, height: auto, margin: 10mm)\n\n= Main\n\n#include \"chapters/intro.typ\"",
                                )
                                .source_file("chapters/intro.typ", "Included from an explicit Project File.")
                                .build()
                                .expect("project with explicit files should be valid");
                            renderer.write().render(&project, &environment, RenderFormat::Html);
                        },
                        "Render the two-file project"
                    }
                }
            },
            secondary: rsx! {
                TypstPreview { render: renderer }
            },
        }
    }
}
