//! Support helpers for the Dioxus Typst demo UI (`web`/`server` targets).

use diotypst::{
    DocumentWorkspace, DownloadError, FileImportError, ImportedProjectFile, PackageBundle,
    PackagePreparationStatus, ProjectPack, ProjectPackMetadata, RenderDiagnostic, RenderError,
    RenderSourceRange, RenderStatus, VirtualPath, WorkspaceValidationError, WorldPreparationPhase,
    WorldPreparationState,
};
use std::collections::HashSet;

pub const SAMPLE_TYPST: &str = r#"#set page(width: 120mm, height: auto, margin: 10mm)
#set text(font: "Libertinus Serif", size: 11pt)

= Dioxus + Typst

This demo renders semantic HTML from Typst source owned by Dioxus state.

Hello, #sys.inputs.at("name", default: "world")!

#let badge(text) = box(
  inset: 6pt,
  radius: 4pt,
  fill: rgb("eef2ff"),
  text
)

#badge[Explicit render action]
"#;

/// Template exercising a Typst Universe download through the demo allowlist.
pub const CETZ_TEMPLATE: &str = r#"#set page(width: 120mm, height: auto, margin: 10mm)

= CetZ From Typst Universe

#import "@preview/cetz:0.4.2": canvas, draw

#canvas({
  import draw: *
  rect((0, 0), (2, 1), fill: rgb("eef2ff"), radius: 0.1)
  circle((3, 0.5), radius: 0.4, fill: rgb("c7d2fe"))
  line((0, -0.6), (3.6, -0.6))
})
"#;

/// Template decoding a JSON System Input into structured data.
pub const JSON_DATA_TEMPLATE: &str = r#"#set page(width: 120mm, height: auto, margin: 10mm)

#let data = json(bytes(sys.inputs.at("data", default: "{}")))

= #data.at("title", default: "Untitled")

#table(
  columns: (1fr, auto),
  table.header([*Task*], [*Hours*]),
  ..data
    .at("items", default: ())
    .map(item => ([#item.name], [#item.hours]))
    .flatten(),
)

Total: #data.at("items", default: ()).map(item => item.hours).sum(default: 0) hours
"#;

/// Sample data for [`JSON_DATA_TEMPLATE`], valid by construction.
pub const SAMPLE_JSON_DATA: &str = r#"{
  "title": "Sprint plan",
  "items": [
    { "name": "Design review", "hours": 4 },
    { "name": "Implementation", "hours": 16 },
    { "name": "Documentation", "hours": 3 }
  ]
}"#;

/// Template exercising the Package Bundle embedded into the app binary.
pub const EMBEDDED_PACKAGE_TEMPLATE: &str = r#"#set page(width: 120mm, height: auto, margin: 10mm)

= Embedded Package

#import "@demo/demo-badge:0.1.0": badge

This package was embedded into the binary as a verbatim archive.

#badge[Rendered from an embedded Package Bundle]
"#;

/// Template exercising a package the demo Package Policy denies.
pub const DENIED_PACKAGE_TEMPLATE: &str = r#"#set page(width: 120mm, height: auto, margin: 10mm)

= Denied Package

#import "@preview/tablex:0.0.9": tablex

This import is outside the demo allowlist, so preparation reports it as denied.
"#;

pub use crate::packages::demo_package_policy;

/// The demo Package Bundle embedded into the binary as a verbatim `.tar.gz` archive.
pub fn embedded_demo_package() -> PackageBundle {
    PackageBundle::from_tar_gz(
        "@demo/demo-badge:0.1.0"
            .parse()
            .expect("embedded package spec should parse"),
        include_bytes!("includes/demo-badge-0.1.0.tar.gz"),
    )
    .expect("embedded package archive should parse")
}

/// The sample Project Pack for the `.typk` example: the embedded-package
/// template with its `@demo/demo-badge` Package Bundle vendored inside.
pub fn demo_project_pack() -> ProjectPack {
    ProjectPack::builder(DocumentWorkspace::from_source(EMBEDDED_PACKAGE_TEMPLATE))
        .package_bundle(embedded_demo_package())
        .metadata(
            ProjectPackMetadata::new()
                .with_name("diotypst demo badge")
                .with_description("Sample Project Pack built by the diotypst demo"),
        )
        .build()
        .expect("demo pack should build")
}

/// One-line summary of a loaded Project Pack for the demo status area.
pub fn pack_summary(pack: &ProjectPack) -> String {
    let name = pack
        .metadata()
        .and_then(|metadata| metadata.name())
        .unwrap_or("unnamed pack");
    let mut parts = vec![
        format!("{} project files", pack.project().files().len()),
        format!("{} vendored packages", pack.package_bundles().len()),
    ];
    if !pack.external_packages().is_empty() {
        parts.push(format!(
            "{} external packages to resolve",
            pack.external_packages().len()
        ));
    }
    if !pack.font_files().is_empty() {
        parts.push(format!("{} embedded font files", pack.font_files().len()));
    }

    format!("Loaded \"{name}\": {}.", parts.join(", "))
}

pub fn file_import_error_summary(error: &FileImportError) -> String {
    match error {
        FileImportError::Read { name, message } => format!("Could not read {name}: {message}"),
        FileImportError::TooLarge { name, size, limit } => {
            format!("{name} is {size} bytes, over the {limit} byte import limit.")
        }
    }
}

pub fn build_imported_workspace<'a>(
    root_path: impl Into<String>,
    files: impl IntoIterator<Item = &'a ImportedProjectFile>,
) -> Result<DocumentWorkspace, WorkspaceValidationError> {
    let mut builder = DocumentWorkspace::builder(root_path);

    for file in files {
        builder = builder.file(file.path().to_owned(), file.bytes().to_vec());
    }

    builder.build()
}

pub fn typst_root_candidates<'a>(
    files: impl IntoIterator<Item = &'a ImportedProjectFile>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for file in files {
        let Ok(path) = VirtualPath::new(file.path()) else {
            continue;
        };
        let path = path.get_without_slash();
        if path.ends_with(".typ") && seen.insert(path.to_owned()) {
            candidates.push(path.to_owned());
        }
    }

    candidates
}

pub fn preparation_phase_label(phase: WorldPreparationPhase) -> &'static str {
    match phase {
        WorldPreparationPhase::Idle => "idle",
        WorldPreparationPhase::Preparing => "preparing",
        WorldPreparationPhase::Ready => "ready",
        WorldPreparationPhase::Failed => "failed",
    }
}

pub fn package_status_label(status: PackagePreparationStatus) -> &'static str {
    match status {
        PackagePreparationStatus::Queued => "queued",
        PackagePreparationStatus::Downloading => "downloading",
        PackagePreparationStatus::Ready => "ready",
        PackagePreparationStatus::Denied => "denied",
        PackagePreparationStatus::Failed => "failed",
    }
}

/// One-line summary of a World Preparation run for the demo status area.
pub fn preparation_summary(state: &WorldPreparationState) -> String {
    let phase = preparation_phase_label(state.phase());

    if state.packages().is_empty() {
        return format!("packages: {phase}");
    }

    let packages = state
        .packages()
        .iter()
        .map(|entry| {
            format!(
                "{} ({})",
                entry.spec(),
                package_status_label(entry.status())
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("packages: {phase} - {packages}")
}

pub fn html_body_fragment(html: &str) -> &str {
    let Some(body_start) = html.find("<body") else {
        return html;
    };
    let Some(open_end) = html[body_start..].find('>') else {
        return html;
    };
    let fragment_start = body_start + open_end + 1;
    let Some(body_end) = html[fragment_start..].rfind("</body>") else {
        return &html[fragment_start..];
    };

    &html[fragment_start..fragment_start + body_end]
}

pub fn render_status_label(status: RenderStatus) -> &'static str {
    match status {
        RenderStatus::Empty => "not rendered",
        RenderStatus::Current => "current",
        RenderStatus::Stale => "stale",
        RenderStatus::Failed => "failed",
    }
}

pub fn workspace_validation_summary(error: &WorkspaceValidationError) -> String {
    match error {
        WorkspaceValidationError::InvalidPath { path } => format!(
            "Workspace path {path} is invalid. Use root-relative paths inside the imported workspace."
        ),
        WorkspaceValidationError::DuplicatePath { path } => {
            format!("Workspace path {path} appears more than once after normalization.")
        }
        WorkspaceValidationError::MissingRoot { root } => {
            format!("Root entrypoint {root} was not found in the imported workspace.")
        }
    }
}

pub fn render_error_summary(error: &RenderError) -> String {
    match error {
        RenderError::Workspace(error) => workspace_validation_summary(error),
        RenderError::Diagnostics(diagnostics) => diagnostics
            .iter()
            .map(render_diagnostic_summary)
            .collect::<Vec<_>>()
            .join("\n"),
        RenderError::ImageEncoding(message) => format!("Image encoding failed: {message}"),
        RenderError::UnsupportedFormat { format } => {
            format!("This build has no Render Capability for {format:?}.")
        }
        RenderError::UnsupportedTarget { target } => {
            format!("This build has no Render Capability for the {target:?} preflight target.")
        }
    }
}

fn render_diagnostic_summary(diagnostic: &RenderDiagnostic) -> String {
    let location = match (diagnostic.workspace_path(), diagnostic.source_range()) {
        (Some(path), Some(range)) => Some(format!(
            "{}:{}",
            path.get_without_slash(),
            render_source_range_label(range)
        )),
        (Some(path), None) => Some(path.get_without_slash().to_owned()),
        (None, Some(range)) => Some(format!("line {}", render_source_range_label(range))),
        (None, None) => None,
    };

    match location {
        Some(location) => format!("{location}: {}", diagnostic.message()),
        None => diagnostic.message().to_owned(),
    }
}

fn render_source_range_label(range: RenderSourceRange) -> String {
    let start_line = range.start_line() + 1;
    let start_column = range.start_column() + 1;
    let end_line = range.end_line() + 1;
    let end_column = range.end_column() + 1;

    if start_line == end_line && start_column == end_column {
        format!("{start_line}:{start_column}")
    } else if start_line == end_line {
        format!("{start_line}:{start_column}-{end_column}")
    } else {
        format!("{start_line}:{start_column}-{end_line}:{end_column}")
    }
}

pub fn download_error_summary(error: &DownloadError) -> &'static str {
    match error {
        DownloadError::Unavailable => "No current or stale artifact is available to download.",
        DownloadError::UnsupportedArtifact => {
            "HTML artifacts are preview-only and cannot be downloaded."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diotypst::{
        render_html, use_typst_render, DocumentWorkspace, HeadlessRender, RenderArtifact,
        RenderEnvironment, RenderFormat, WorkspaceValidationError,
    };
    use dioxus::prelude::*;
    use std::cell::RefCell;

    thread_local! {
        static HOOK_RENDERER: RefCell<Option<Signal<HeadlessRender>>> = const { RefCell::new(None) };
    }

    fn hook_app() -> Element {
        let renderer = use_typst_render();

        use_hook(move || {
            HOOK_RENDERER.with(|cell| {
                *cell.borrow_mut() = Some(renderer);
            });
        });

        rsx! {}
    }

    fn render_signal_from_hook() -> (VirtualDom, Signal<HeadlessRender>) {
        HOOK_RENDERER.with(|cell| {
            *cell.borrow_mut() = None;
        });

        let mut dom = VirtualDom::new(hook_app);
        dom.rebuild_in_place();
        let renderer = HOOK_RENDERER.with(|cell| {
            cell.borrow_mut()
                .take()
                .expect("hook should expose a HeadlessRender signal")
        });

        (dom, renderer)
    }

    #[test]
    fn dioxus_render_hook_exposes_current_stale_and_failed_render_state() {
        let (_dom, mut renderer) = render_signal_from_hook();
        let environment = RenderEnvironment::builder()
            .build()
            .expect("render environment should be valid");
        let current_workspace = DocumentWorkspace::from_source(SAMPLE_TYPST);
        let broken_workspace = DocumentWorkspace::from_source("#let broken =");

        renderer
            .write()
            .render(&current_workspace, &environment, RenderFormat::Html);

        {
            let renderer = renderer.read();
            let state = renderer.state();
            assert_eq!(state.status(), RenderStatus::Current);
            let Some(RenderArtifact::Html(html)) = state.artifact() else {
                panic!("expected HTML artifact");
            };
            assert!(html.as_str().contains("Dioxus + Typst"));
            assert!(state.error().is_none());
        }

        renderer
            .write()
            .render(&broken_workspace, &environment, RenderFormat::Html);

        {
            let renderer = renderer.read();
            let state = renderer.state();
            assert_eq!(state.status(), RenderStatus::Stale);
            let Some(RenderArtifact::Html(html)) = state.artifact() else {
                panic!("expected stale HTML artifact");
            };
            assert!(html.as_str().contains("Dioxus + Typst"));
            assert!(state.error().is_some());
        }

        let (_failed_dom, mut failed_renderer) = render_signal_from_hook();
        failed_renderer
            .write()
            .render(&broken_workspace, &environment, RenderFormat::Html);

        let failed_renderer = failed_renderer.read();
        let failed_state = failed_renderer.state();
        assert_eq!(failed_state.status(), RenderStatus::Failed);
        assert!(failed_state.artifact().is_none());
        assert!(failed_state.error().is_some());
    }

    #[test]
    fn demo_render_handle_keeps_rendered_html() {
        let environment = RenderEnvironment::builder()
            .build()
            .expect("render environment should be valid");
        let workspace = DocumentWorkspace::from_source(SAMPLE_TYPST);
        let mut renderer = HeadlessRender::new();

        renderer.render(&workspace, &environment, RenderFormat::Html);

        let state = renderer.state();
        assert_eq!(state.status(), RenderStatus::Current);
        let Some(RenderArtifact::Html(html)) = state.artifact() else {
            panic!("expected HTML artifact");
        };
        assert!(html.as_str().contains("Dioxus + Typst"));
    }

    #[test]
    fn demo_render_handle_keeps_stale_html_after_error() {
        let environment = RenderEnvironment::builder()
            .build()
            .expect("render environment should be valid");
        let current_workspace = DocumentWorkspace::from_source(SAMPLE_TYPST);
        let broken_workspace = DocumentWorkspace::from_source("#let broken =");
        let mut renderer = HeadlessRender::new();

        renderer.render(&current_workspace, &environment, RenderFormat::Html);
        renderer.render(&broken_workspace, &environment, RenderFormat::Html);

        let state = renderer.state();
        assert_eq!(state.status(), RenderStatus::Stale);
        let Some(RenderArtifact::Html(html)) = state.artifact() else {
            panic!("expected stale HTML artifact");
        };
        assert!(html.as_str().contains("Dioxus + Typst"));
        assert!(
            render_error_summary(state.error().expect("failed render should be recorded"))
                .contains("expected expression")
        );
    }

    #[test]
    fn demo_render_error_summary_includes_diagnostic_file_and_line() {
        let environment = RenderEnvironment::builder()
            .build()
            .expect("render environment should be valid");
        let workspace = DocumentWorkspace::from_source("= Title\n\n#let broken =");
        let error = render_html(&workspace, &environment)
            .expect_err("invalid Typst source should fail with diagnostics");

        let summary = render_error_summary(&error);

        assert!(summary.starts_with("main.typ:3:"), "{summary}");
        assert!(summary.contains("expected expression"), "{summary}");
    }

    #[test]
    fn demo_html_preview_uses_body_fragment() {
        let environment = RenderEnvironment::builder()
            .build()
            .expect("render environment should be valid");
        let workspace = DocumentWorkspace::from_source(SAMPLE_TYPST);
        let mut renderer = HeadlessRender::new();

        renderer.render(&workspace, &environment, RenderFormat::Html);

        let Some(RenderArtifact::Html(html)) = renderer.state().artifact() else {
            panic!("expected HTML artifact");
        };
        let html = html.as_str();
        let fragment = html_body_fragment(html);

        assert!(html.starts_with("<!DOCTYPE html>\n<html"));
        assert!(fragment.contains("Dioxus + Typst"));
        assert!(!fragment.contains("<html>"));
        assert!(!fragment.contains("<body>"));
    }

    #[test]
    fn demo_download_errors_are_user_facing() {
        assert_eq!(
            download_error_summary(&diotypst::DownloadError::Unavailable),
            "No current or stale artifact is available to download."
        );
        assert_eq!(
            download_error_summary(&diotypst::DownloadError::UnsupportedArtifact),
            "HTML artifacts are preview-only and cannot be downloaded."
        );
    }

    #[test]
    fn imported_files_build_a_renderable_document_workspace() {
        let files = [
            ImportedProjectFile::new(
                "main.typ",
                "= Imported\n\n#include \"chapters/intro.typ\"",
                Some("text/typst".to_owned()),
            ),
            ImportedProjectFile::new(
                "chapters/./intro.typ",
                "Included from the imported workspace.",
                Some("text/typst".to_owned()),
            ),
        ];
        let environment = RenderEnvironment::builder()
            .build()
            .expect("render environment should be valid");

        let workspace = build_imported_workspace("main.typ", &files)
            .expect("imported workspace should be valid");
        let html = render_html(&workspace, &environment)
            .expect("imported workspace should render through the sandbox");

        assert_eq!(workspace.root_path().get_without_slash(), "main.typ");
        assert_eq!(
            workspace.file_bytes("chapters/intro.typ"),
            Some("Included from the imported workspace.".as_bytes())
        );
        assert!(html
            .as_str()
            .contains("<p>Included from the imported workspace.</p>"));
    }

    #[test]
    fn import_root_candidates_are_normalized_typst_files() {
        let files = [
            ImportedProjectFile::new(
                "chapters/./intro.typ",
                "= Intro",
                Some("text/typst".to_owned()),
            ),
            ImportedProjectFile::new(
                "assets/logo.png",
                vec![0x89, 0x50],
                Some("image/png".to_owned()),
            ),
            ImportedProjectFile::new("../secret.typ", "= Secret", None),
            ImportedProjectFile::new("main.typ", "= Main", Some("text/typst".to_owned())),
        ];

        let candidates = typst_root_candidates(&files);

        assert_eq!(candidates, ["chapters/intro.typ", "main.typ"]);
    }

    #[test]
    fn embedded_demo_package_renders_through_the_environment() {
        let environment = RenderEnvironment::builder()
            .package_bundle(embedded_demo_package())
            .build()
            .expect("environment with the embedded package should build");
        let workspace = DocumentWorkspace::from_source(EMBEDDED_PACKAGE_TEMPLATE);

        let html =
            render_html(&workspace, &environment).expect("embedded package template should render");

        assert!(html
            .as_str()
            .contains("Rendered from an embedded Package Bundle"));
    }

    #[test]
    fn demo_project_pack_round_trips_and_renders_offline() {
        let bytes = demo_project_pack()
            .to_bytes()
            .expect("demo pack should serialize");
        let pack = ProjectPack::from_bytes(&bytes).expect("demo pack should parse back");
        let environment = pack
            .render_environment()
            .expect("pack environment should build");

        let html =
            render_html(pack.project(), &environment).expect("loaded pack should render offline");

        assert!(html
            .as_str()
            .contains("Rendered from an embedded Package Bundle"));
        assert_eq!(
            pack_summary(&pack),
            "Loaded \"diotypst demo badge\": 1 project files, 1 vendored packages."
        );
    }

    #[test]
    fn json_data_template_renders_with_the_sample_input() {
        let environment = RenderEnvironment::builder()
            .input("data", SAMPLE_JSON_DATA)
            .build()
            .expect("environment with a JSON input should build");
        let workspace = DocumentWorkspace::from_source(JSON_DATA_TEMPLATE);

        let html = render_html(&workspace, &environment).expect("JSON data template should render");

        assert!(html.as_str().contains("Sprint plan"));
        assert!(html.as_str().contains("Implementation"));
        assert!(html.as_str().contains("23"), "hours should sum to 23");
    }

    #[test]
    fn preparation_summaries_are_user_facing() {
        let environment = RenderEnvironment::builder()
            .build()
            .expect("empty environment should build");
        let mut state = WorldPreparationState::new(environment);
        assert_eq!(preparation_summary(&state), "packages: idle");

        state.begin();
        let spec = "@preview/cetz:0.4.2"
            .parse::<diotypst::PackageSpec>()
            .expect("spec should parse");
        state.queue(&spec);
        state.set_status(&spec, PackagePreparationStatus::Downloading, None);

        assert_eq!(
            preparation_summary(&state),
            "packages: preparing - @preview/cetz:0.4.2 (downloading)"
        );
    }

    #[test]
    fn workspace_validation_errors_are_user_facing() {
        assert_eq!(
            workspace_validation_summary(&WorkspaceValidationError::InvalidPath {
                path: "../secret.typ".to_owned(),
            }),
            "Workspace path ../secret.typ is invalid. Use root-relative paths inside the imported workspace."
        );
        assert_eq!(
            workspace_validation_summary(&WorkspaceValidationError::DuplicatePath {
                path: "chapters/intro.typ".to_owned(),
            }),
            "Workspace path chapters/intro.typ appears more than once after normalization."
        );
        assert_eq!(
            workspace_validation_summary(&WorkspaceValidationError::MissingRoot {
                root: "main.typ".to_owned(),
            }),
            "Root entrypoint main.typ was not found in the imported workspace."
        );
    }
}
