use diotypst::{
    FileImportOptions, ImportedProjectFile, RenderFormat, import_project_files,
    partition_imported_fonts, use_typst_defaults, use_typst_render,
};
use dioxus::prelude::*;

use super::TypstPreview;
use crate::components::{
    ControlEmphasis, DemoPane, DemoSurface, FilePickerButton, FilePickerKind, StatusLine,
};
use crate::{
    build_imported_workspace, file_import_error_summary, typst_root_candidates,
    workspace_validation_summary,
};

/// Import browser file selections through the shared Dioxus file abstraction:
/// every file becomes an explicit Project File, except font files, which join
/// the render Font Set instead. `.typ` files become Root Entrypoint candidates.
async fn import_selection(
    selection: Vec<dioxus::html::FileData>,
    mut files: Signal<Vec<ImportedProjectFile>>,
    mut root_path: Signal<String>,
    mut fonts: Signal<Vec<Vec<u8>>>,
    mut message: Signal<String>,
) {
    match import_project_files(selection, FileImportOptions::default()).await {
        Ok(imported) => {
            let (project_files, font_files) = partition_imported_fonts(imported);
            message.set(match font_files.len() {
                0 => String::new(),
                count => format!("Added {count} font files to the render Font Set."),
            });
            fonts.write().extend(font_files);

            if !project_files.is_empty() {
                let roots = typst_root_candidates(&project_files);
                root_path.set(
                    roots
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "main.typ".to_owned()),
                );
                files.set(project_files);
            }
        }
        Err(error) => message.set(file_import_error_summary(&error)),
    }
}

#[component]
pub fn ImportExample() -> Element {
    let files = use_signal(Vec::<ImportedProjectFile>::new);
    let mut root_path = use_signal(String::new);
    let mut fonts = use_signal(Vec::<Vec<u8>>::new);
    let mut message = use_signal(String::new);
    let mut renderer = use_typst_render();
    let base_environment = use_typst_defaults().environment().clone();

    let imported = files.read().clone();
    let roots = typst_root_candidates(&imported);
    let font_count = fonts.read().len();

    rsx! {
        DemoSurface {
            primary: rsx! {
                DemoPane { label: "Live",
                    div { class: "flex flex-wrap items-center gap-2",
                        FilePickerButton {
                            label: "Choose files",
                            kind: FilePickerKind::Multiple,
                            emphasis: ControlEmphasis::Primary,
                            onpick: move |selection: Vec<dioxus::html::FileData>| async move {
                                import_selection(selection, files, root_path, fonts, message).await;
                            },
                        }
                        FilePickerButton {
                            label: "Choose a directory",
                            kind: FilePickerKind::Directory,
                            onpick: move |selection: Vec<dioxus::html::FileData>| async move {
                                import_selection(selection, files, root_path, fonts, message).await;
                            },
                        }
                        if font_count > 0 {
                            button {
                                class: "btn btn-sm btn-ghost",
                                onclick: move |_| fonts.set(Vec::new()),
                                "Clear fonts ({font_count})"
                            }
                        }
                    }
                    if !imported.is_empty() {
                        label { class: "mt-3 block space-y-1",
                            span { class: "text-sm font-medium", "Root Entrypoint" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{root_path}",
                                onchange: move |event| root_path.set(event.value()),
                                for candidate in roots {
                                    option { value: "{candidate}", "{candidate}" }
                                }
                            }
                        }
                        ul { class: "mt-2 space-y-0.5 font-mono text-xs text-base-content/65",
                            for file in imported.iter().take(6) {
                                li { "{file.path()} ({file.bytes().len()} bytes)" }
                            }
                            if imported.len() > 6 {
                                li { "+{imported.len() - 6} more files" }
                            }
                        }
                        button {
                            class: "btn btn-sm btn-primary mt-3",
                            onclick: move |_| {
                                let project =
                                    build_imported_workspace(root_path.read().clone(), files.read().iter());
                                let font_set = base_environment
                                    .font_set()
                                    .clone()
                                    .with_font_files(fonts.read().clone());
                                let environment = base_environment
                                    .to_builder()
                                    .font_set(font_set)
                                    .build()
                                    .expect("render environment with imported fonts should be valid");

                                match project {
                                    Ok(project) => {
                                        renderer.write().render(&project, &environment, RenderFormat::Html)
                                    }
                                    Err(error) => message.set(workspace_validation_summary(&error)),
                                }
                            },
                            "Render the imported project"
                        }
                    }
                    StatusLine { status: message }
                }
            },
            secondary: rsx! {
                TypstPreview { render: renderer }
            },
        }
    }
}
