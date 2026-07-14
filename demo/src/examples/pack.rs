use crate::{demo_project_pack, pack_summary};
use diotypst::{
    DownloadFile, ProjectPack, RenderFormat, trigger_browser_download, use_typst_defaults,
    use_typst_render,
};
use dioxus::prelude::*;

use super::TypstPreview;
use crate::components::{DemoPane, DemoSurface, FilePickerButton, FilePickerKind, StatusLine};

/// Round trip a Typst Project through a portable `.typk` Project Pack. The
/// download side serializes the project with its `@demo/demo-badge` Package
/// Bundle vendored inside; loading a pack back renders fully offline: no
/// Package Source, no World Preparation, no network.
#[component]
pub fn PackExample() -> Element {
    let mut renderer = use_typst_render();
    let mut status = use_signal(String::new);
    let base_environment = use_typst_defaults().environment().clone();

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane { label: "Live",
                    div { class: "flex flex-wrap items-center gap-2",
                        button {
                            class: "btn btn-sm btn-primary",
                            onclick: move |_| {
                                match DownloadFile::from_project_pack("demo-badge.typk", &demo_project_pack()) {
                                    Ok(file) => match trigger_browser_download(&file) {
                                        Ok(()) => status.set(format!(
                                            "Downloaded {} ({} bytes). Load it back with \"Load a .typk\".",
                                            file.filename(),
                                            file.bytes().len(),
                                        )),
                                        Err(error) => status.set(format!("Browser download failed: {error:?}")),
                                    },
                                    Err(error) => status.set(format!("Packing failed: {error:?}")),
                                }
                            },
                            "Build & download .typk"
                        }
                        FilePickerButton {
                            label: "Load a .typk",
                            kind: FilePickerKind::Single,
                            accept: ".typk",
                            onpick: move |files: Vec<dioxus::html::FileData>| {
                                let base_environment = base_environment.clone();
                                async move {
                                    let Some(file) = files.into_iter().next() else {
                                        return;
                                    };
                                    let bytes = match file.read_bytes().await {
                                        Ok(bytes) => bytes,
                                        Err(error) => {
                                            return status.set(format!("Could not read {}: {error}", file.name()));
                                        }
                                    };
                                    match ProjectPack::from_bytes(&bytes) {
                                        Ok(pack) => {
                                            let font_set = base_environment
                                                .font_set()
                                                .clone()
                                                .with_font_files(pack.font_files().iter().cloned());
                                            let environment = base_environment
                                                .to_builder()
                                                .package_bundles(pack.package_bundles().iter().cloned())
                                                .font_set(font_set)
                                                .build()
                                                .expect("pack environment should be valid");
                                            renderer
                                                .write()
                                                .render(pack.project(), &environment, RenderFormat::Html);
                                            status.set(pack_summary(&pack));
                                        }
                                        Err(error) => status.set(format!("Not a readable pack: {error:?}")),
                                    }
                                }
                            },
                        }
                    }
                    StatusLine { status }
                }
            },
            secondary: rsx! {
                TypstPreview { render: renderer }
            },
        }
    }
}
