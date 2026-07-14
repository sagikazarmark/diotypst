use dioxus::prelude::*;
use dioxus_code::{Code, code};

use crate::components::{DocsCallout, ExampleSection, InlineCode, PageHeader, snippet_theme};
use crate::examples::import::ImportExample;
use crate::examples::multi_file::MultiFileExample;
use crate::examples::pack::PackExample;

#[component]
pub fn MultiFile() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Typst Projects",
            title: "A multi-file project",
            intro: "A Typst Project is one Root Entrypoint plus explicit Project Files addressed by root-relative Project Paths. Includes, images, and data loading resolve against those files only.",
        }
        ExampleSection {
            title: "DocumentWorkspace::builder",
            intro: rsx! {
                "The builder collects "
                InlineCode { "source_file" }
                " and binary "
                InlineCode { "file" }
                " entries and validates paths on "
                InlineCode { "build" }
                ": parent-directory escapes and duplicate normalized paths are rejected before anything renders."
            },
            demo: rsx! { MultiFileExample {} },
            code: rsx! {
                Code { src: code!("src/examples/multi_file.rs"), theme: snippet_theme() }
            },
        }
    }
}

#[component]
pub fn ImportProject() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Typst Projects",
            title: "Import files & fonts",
            intro: "Browser file selections become explicit Project Files through the shared Dioxus file abstraction. Font files are split out into the render Font Set instead of the project; .typ files become Root Entrypoint candidates.",
        }
        ExampleSection {
            title: "import_project_files + partition_imported_fonts",
            intro: rsx! {
                "Pick a directory when includes or assets depend on nested paths. The imported project renders through the same explicit Project World as inline source: the render still cannot read anything you did not import."
            },
            demo: rsx! { ImportExample {} },
            code: rsx! {
                Code { src: code!("src/examples/import.rs"), theme: snippet_theme() }
            },
        }
        DocsCallout {
            title: "Closed-world rendering",
            "Rendering sees only the Typst Project, prepared Package Bundles, the configured Font Set, and the configured render date. Imports here feed that sandbox; they never widen it."
        }
    }
}

#[component]
pub fn ProjectPacks() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Typst Projects",
            title: "Portable project packs (.typk)",
            intro: "A Project Pack is a single .typk archive of a whole Typst Project: sources, assets, vendored Package Bundles, and optional embedded fonts. Packs read and write in the browser, so a project can leave one app and render offline in another.",
        }
        ExampleSection {
            title: "ProjectPack::builder + ProjectPack::from_bytes",
            intro: rsx! {
                "Build a pack with the "
                InlineCode { "@demo/demo-badge" }
                " bundle vendored inside, download it, then load it back: the loaded pack supplies the Typst Project and a complete Render Environment, so rendering needs no Package Source and no network."
            },
            demo: rsx! { PackExample {} },
            code: rsx! {
                Code { src: code!("src/examples/pack.rs"), theme: snippet_theme() }
            },
        }
        DocsCallout {
            title: "The .typk format",
            "The format is defined by the independent typst-pack crate: a Zip archive with a typst-pack.toml manifest, project/ files, packages/ for vendored dependencies, and fonts/ for embedded fonts. Vendored Typst Universe packages stay verbatim .tar.gz archives on the registry side; a pack is the project-level exchange format."
        }
    }
}
