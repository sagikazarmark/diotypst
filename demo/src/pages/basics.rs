use dioxus::prelude::*;
use dioxus_code::{Code, code};

use crate::app::Route;
use crate::components::{
    DocsCallout, ExampleLayout, ExampleSection, ExternalAction, InlineCode, PageHeader,
    snippet_theme,
};
use crate::examples::editor::EditorExample;
use crate::examples::json_inputs::JsonInputsExample;
use crate::examples::minimal::MinimalExample;
use crate::examples::sys_inputs::SysInputsExample;

#[component]
pub fn Home() -> Element {
    let groups = [
        (
            "Basics",
            "Explicit rendering, a debounced live preview, and System Inputs.",
            Route::Minimal {},
        ),
        (
            "Typst Projects",
            "Multi-file projects with explicit files, plus browser file and font import.",
            Route::MultiFile {},
        ),
        (
            "Packages",
            "Typst Universe downloads through a policy-gated proxy, plus embedded bundles.",
            Route::UniversePackages {},
        ),
        (
            "Downloads",
            "Client-side PDF and Page Image Archive downloads from render state.",
            Route::PdfDownload {},
        ),
        (
            "Server",
            "Server-rendered downloads through the fullstack Server Render Route.",
            Route::ServerRendering {},
        ),
    ];

    rsx! {
        PageHeader {
            eyebrow: "diotypst",
            title: "Render Typst from Dioxus state",
            intro: "Typst integration primitives for Dioxus apps: an explicit Typst Project rendered inside an explicit Render Environment through a crate-owned Project World. Nothing reads the host filesystem or fetches packages implicitly. Every page here mounts a real feature next to the exact source that runs it.",
        }

        div { class: "mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3",
            for (title , blurb , route) in groups {
                Link {
                    to: route,
                    class: "group rounded-2xl border border-base-300 bg-base-100 p-5 shadow-sm transition-colors hover:border-primary/40 hover:bg-base-200/40",
                    p { class: "font-semibold tracking-tight group-hover:text-primary", "{title}" }
                    p { class: "mt-1 text-sm text-base-content/65", "{blurb}" }
                }
            }
        }

        DocsCallout {
            title: "Start with the README",
            action: Some(ExternalAction::new(
                "Repository & docs",
                "https://github.com/sagikazarmark/diotypst",
            )),
            "The workspace is split into typst-project (Dioxus-independent Project World construction) and diotypst (render hooks, components, downloads, package preparation, and the fullstack server routes). Design terminology lives in CONTEXT.md."
        }
    }
}

#[component]
pub fn Minimal() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Basics",
            title: "A minimal render",
            intro: "The smallest complete flow: one Render Session over a Typst Project from one source string, rendering a semantic HTML artifact on mount.",
        }
        ExampleSection {
            title: "use_render_session",
            // The demo is a lone preview and the source is short, so the
            // side-by-side layout beats tabs here.
            layout: ExampleLayout::Columns,
            intro: rsx! {
                InlineCode { "use_render_session" }
                " keeps the latest Render Artifact, Stale Artifacts, and diagnostics in one "
                InlineCode { "HeadlessRender" }
                " state. The session is declarative: it renders on mount and re-renders whenever the input it reads changes. For a constant source there is nothing to trigger; rendering is deterministic, so re-rendering unchanged input would produce the same artifact."
            },
            demo: rsx! { MinimalExample {} },
            code: rsx! {
                Code { src: code!("src/examples/minimal.rs"), theme: snippet_theme() }
            },
        }
    }
}

#[component]
pub fn Editor() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Basics",
            title: "Editor & live preview",
            intro: "The Render Policy is signal wiring: the editor shows the live text while the session reads a second signal, set on an explicit Render action or through a debounced live preview.",
        }
        ExampleSection {
            title: "Two signals: live editor, session input",
            intro: rsx! {
                "The session renders whatever its input signal holds. Explicit mode commits the editor text on Render now; live preview routes edits through "
                InlineCode { "use_debounce" }
                " (dioxus-sdk-time), so the render follows 400 ms behind. Introduce an error (delete a closing bracket) to see the "
                InlineCode { "stale" }
                " status: the last good artifact stays visible while diagnostics point at the broken source."
            },
            demo: rsx! { EditorExample {} },
            code: rsx! {
                Code { src: code!("src/examples/editor.rs"), theme: snippet_theme() }
            },
        }
    }
}

#[component]
pub fn SysInputs() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Basics",
            title: "System Inputs",
            intro: "System Inputs are explicit Typst values installed into the Render Environment and visible to document code through sys.inputs: no environment variables, no ambient application state.",
        }
        ExampleSection {
            title: "RenderEnvironment::builder().input(…)",
            intro: rsx! {
                "Type a name; the debounced value rebuilds the environment with "
                InlineCode { "input(\"name\", …)" }
                " and the session re-renders. The document reads it with "
                InlineCode { "sys.inputs.at(\"name\", default: \"world\")" }
                "."
            },
            demo: rsx! { SysInputsExample {} },
            code: rsx! {
                Code { src: code!("src/examples/sys_inputs.rs"), theme: snippet_theme() }
            },
        }
        ExampleSection {
            title: "Structured data as JSON",
            intro: rsx! {
                "System Inputs are strings, so structured data travels as JSON: the app validates it with "
                InlineCode { "serde_json" }
                " before it reaches the Render Environment, and the document decodes it with "
                InlineCode { "json(bytes(sys.inputs.at(\"data\")))" }
                ". Break the JSON (delete a quote) to see both layers: the parse error under the editor, and the preview holding the last good artifact while Typst reports its own diagnostics."
            },
            demo: rsx! { JsonInputsExample {} },
            code: rsx! {
                Code { src: code!("src/examples/json_inputs.rs"), theme: snippet_theme() }
            },
        }
    }
}
