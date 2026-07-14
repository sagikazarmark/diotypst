use diotypst::{DocumentWorkspace, RenderFormat, use_typst_defaults, use_typst_render};
use dioxus::prelude::*;

use super::TypstPreview;
use crate::components::{DemoPane, DemoSurface};
use crate::{EMBEDDED_PACKAGE_TEMPLATE, embedded_demo_package};

/// A Package Bundle parsed from a verbatim `.tar.gz` archive embedded into the
/// binary with `include_bytes!` (see `embedded_demo_package`). Installing it
/// into the Render Environment makes `@demo/demo-badge` importable offline:
/// no download, no World Preparation.
#[component]
pub fn EmbeddedExample() -> Element {
    let mut renderer = use_typst_render();
    let base_environment = use_typst_defaults().environment().clone();

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane { label: "Live",
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let project = DocumentWorkspace::from_source(EMBEDDED_PACKAGE_TEMPLATE);
                            let environment = base_environment
                                .to_builder()
                                .package_bundle(embedded_demo_package())
                                .build()
                                .expect("environment with the embedded package should be valid");

                            renderer.write().render(&project, &environment, RenderFormat::Html);
                        },
                        "Render with the embedded package"
                    }
                }
            },
            secondary: rsx! {
                TypstPreview { render: renderer }
            },
        }
    }
}
