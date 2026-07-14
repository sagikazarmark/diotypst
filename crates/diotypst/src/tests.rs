use super::{
    DocumentWorkspace, DownloadError, DownloadFile, DownloadFormat, FontSet, HeadlessRender,
    MemoryPackages, PackageBundle, PackageBundleError, PackageDependencyTarget,
    PackageResolveError, PackageSpec, PageImageOptions, RenderArtifact, RenderDate,
    RenderDownloadError, RenderEnvironment, RenderEnvironmentError, RenderError, RenderFormat,
    RenderState, RenderStatus, SandboxedWorld, ServerRenderRequest, SyncPackageSource,
    WorkspaceFile, WorkspaceValidationError, WorldOverlay, observe_package_dependencies,
    observe_package_dependencies_world, render_artifact, render_artifact_world, render_download,
    render_html, render_html_world, render_page_images, render_page_images_world, render_pdf,
    render_pdf_world,
};
use std::str::FromStr;
use typst::text::FontInfo;

const README: &str = include_str!("../README.md");

// Doctests execute README examples; these checks protect issue #11's ordering and visibility requirements.
#[test]
fn readme_starts_with_a_complete_document_workspace_render_flow() {
    let first_rust_example = README
        .split("```rust\n")
        .nth(1)
        .and_then(|example| example.split("\n```").next())
        .expect("README should include a Rust quickstart example");

    assert!(first_rust_example.contains("DocumentWorkspace::from_source"));
    assert!(first_rust_example.contains("RenderEnvironment::builder()"));
    assert!(first_rust_example.contains("render_pdf("));
    assert!(first_rust_example.contains("starts_with(b\"%PDF-\")"));
}

#[test]
fn readme_keeps_render_constraints_visible() {
    for expected in [
        "explicit Project World",
        "exact package versions",
        "HTML artifacts are preview-only and cannot be downloaded",
    ] {
        assert!(
            README.contains(expected),
            "README should document {expected}"
        );
    }
}

#[test]
fn typst_project_alias_builds_the_existing_project_model() {
    let project = DocumentWorkspace::from_source("= Title");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    assert_eq!(project.root_path().get_without_slash(), "main.typ");
    assert_eq!(project.file_bytes("main.typ"), Some("= Title".as_bytes()));
    SandboxedWorld::new(project, environment).expect("project world should be valid");
}

#[test]
fn source_text_creates_a_valid_document_workspace() {
    let workspace = DocumentWorkspace::from_source("= Title");

    assert_eq!(workspace.root_path().get_without_slash(), "main.typ");
    assert_eq!(workspace.file_bytes("main.typ"), Some("= Title".as_bytes()));
    assert!(workspace.validate().is_ok());
}

#[test]
fn custom_root_source_creates_a_valid_document_workspace() {
    let workspace = DocumentWorkspace::from_source_file("chapters/intro.typ", "= Intro")
        .expect("custom root source should be valid");

    assert_eq!(
        workspace.root_path().get_without_slash(),
        "chapters/intro.typ"
    );
    assert_eq!(
        workspace.file_bytes("chapters/intro.typ"),
        Some("= Intro".as_bytes())
    );
}

#[test]
fn checked_workspace_files_build_a_document_workspace() {
    let root = WorkspaceFile::source("main.typ", "#include \"chapter.typ\"")
        .expect("root path should be valid");
    let chapter =
        WorkspaceFile::source("chapter.typ", "= Chapter").expect("chapter path should be valid");

    let workspace = DocumentWorkspace::new("main.typ", [root, chapter])
        .expect("checked files should build a valid workspace");

    assert_eq!(workspace.files().len(), 2);
    assert!(workspace.contains_path("./chapter.typ"));
}

#[test]
fn workspace_validation_requires_the_root_entrypoint_to_exist() {
    let error = DocumentWorkspace::builder("main.typ")
        .source_file("chapter.typ", "= Chapter")
        .build()
        .expect_err("workspace without its root entrypoint should fail validation");

    assert_eq!(
        error,
        WorkspaceValidationError::MissingRoot {
            root: "main.typ".to_owned(),
        }
    );
}

#[test]
fn workspace_validation_rejects_paths_that_escape_the_workspace_sandbox() {
    let error = DocumentWorkspace::builder("main.typ")
        .source_file("../secret.typ", "= Title")
        .build()
        .expect_err("workspace paths must stay inside the sandbox");

    assert_eq!(
        error,
        WorkspaceValidationError::InvalidPath {
            path: "../secret.typ".to_owned(),
        }
    );
}

#[test]
fn workspace_paths_normalize_rooted_paths() {
    let workspace = DocumentWorkspace::builder("/main.typ")
        .source_file("main.typ", "= Title")
        .build()
        .expect("rooted paths should normalize to workspace paths");

    assert_eq!(workspace.root_path().get_without_slash(), "main.typ");
}

#[test]
fn workspace_validation_rejects_empty_paths() {
    let error = DocumentWorkspace::builder("")
        .source_file("main.typ", "= Title")
        .build()
        .expect_err("empty workspace paths should fail validation");

    assert_eq!(
        error,
        WorkspaceValidationError::InvalidPath {
            path: "".to_owned(),
        }
    );
}

#[test]
fn workspace_validation_rejects_duplicate_workspace_paths() {
    let error = DocumentWorkspace::builder("main.typ")
        .source_file("main.typ", "= First")
        .source_file("main.typ", "= Second")
        .build()
        .expect_err("duplicate workspace paths should fail validation");

    assert_eq!(
        error,
        WorkspaceValidationError::DuplicatePath {
            path: "main.typ".to_owned(),
        }
    );
}

#[test]
fn binary_workspace_files_are_retrievable_by_normalized_path() {
    let workspace = DocumentWorkspace::builder("main.typ")
        .source_file("main.typ", r#"#image("assets/logo.png")"#)
        .file("assets/./logo.png", vec![0x89, 0x50, 0x4e, 0x47])
        .build()
        .expect("workspace should be valid");

    assert_eq!(
        workspace.file_bytes("assets/logo.png"),
        Some(&[0x89, 0x50, 0x4e, 0x47][..])
    );
}

#[test]
fn document_workspace_overlay_files_replace_exact_paths() {
    let base = DocumentWorkspace::builder("main.typ")
        .source_file("main.typ", "#include \"content.typ\"")
        .source_file("content.typ", "Base")
        .build()
        .expect("base workspace should be valid");
    let overlay =
        WorkspaceFile::source("./content.typ", "Overlay").expect("overlay path should be valid");

    let workspace = base.overlay_files([overlay]);

    assert_eq!(
        workspace.file_bytes("content.typ"),
        Some("Overlay".as_bytes())
    );
}

#[test]
fn exact_typst_package_spec_parses_and_exposes_identity() {
    let spec =
        PackageSpec::from_str("@preview/cetz:0.4.2").expect("exact package spec should parse");

    assert_eq!(spec.to_string(), "@preview/cetz:0.4.2");
    assert_eq!(spec.namespace, "preview");
    assert_eq!(spec.name, "cetz");
    assert_eq!(spec.version.to_string(), "0.4.2");
}

#[test]
fn package_spec_rejects_non_exact_or_latest_references() {
    let cases = [
        "preview/cetz:0.4.2",
        "@preview/cetz",
        "@preview/cetz:0.4",
        "@preview/cetz:latest",
        "@preview/cetz:latest/foo",
        "@preview/cetz:0.4.2/extra",
    ];

    for spec in cases {
        PackageSpec::from_str(spec)
            .expect_err("package specs must include exact namespace, name, and version");
    }
}

#[test]
fn package_bundle_stores_files_by_exact_spec_and_normalized_path() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec.clone())
        .file("src/./lib.typ", b"#let answer = 42".to_vec())
        .build()
        .expect("package bundle should be valid");

    assert_eq!(bundle.spec(), &spec);
    assert_eq!(
        bundle.file_bytes("src/lib.typ"),
        Some(b"#let answer = 42".as_slice())
    );
}

#[test]
fn package_bundle_rejects_paths_that_escape_the_bundle() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let error = PackageBundle::builder(spec)
        .file("../src/lib.typ", b"#let answer = 42".to_vec())
        .build()
        .expect_err("package bundle paths must be root-relative and sandboxed");

    assert_eq!(
        error,
        PackageBundleError::InvalidPath {
            path: "../src/lib.typ".to_owned(),
        }
    );
}

#[test]
fn package_bundle_rejects_duplicate_package_file_paths() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let error = PackageBundle::builder(spec)
        .file("src/lib.typ", b"#let first = 1".to_vec())
        .file("src/./lib.typ", b"#let second = 2".to_vec())
        .build()
        .expect_err("duplicate package file paths should fail validation");

    assert_eq!(
        error,
        PackageBundleError::DuplicatePath {
            path: "src/lib.typ".to_owned(),
        }
    );
}

#[test]
fn package_source_resolves_exact_specs_to_package_bundles() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec.clone())
        .file("lib.typ", b"#let answer = 42".to_vec())
        .build()
        .expect("package bundle should be valid");
    let source = MemoryPackages::new([bundle]).expect("package source should be valid");

    let resolved = source
        .resolve_sync(&spec)
        .expect("exact package spec should resolve to a bundle");

    assert_eq!(resolved.spec(), &spec);
    assert_eq!(
        resolved.file_bytes("lib.typ"),
        Some(b"#let answer = 42".as_slice())
    );
}

#[test]
fn package_specs_reject_non_exact_references() {
    let cases = [
        "preview/example:1.2.3",
        "@preview/example",
        "@preview/example:1.2",
        "@preview/example:latest",
        "@preview/example:latest/foo",
        "@preview/example:1.2.3/extra",
    ];

    for spec in cases {
        PackageSpec::from_str(spec).expect_err("non-exact package references should be rejected");
    }
}

#[test]
fn package_source_reports_missing_exact_package_specs() {
    let spec = PackageSpec::from_str("@preview/missing:1.0.0").expect("spec should parse");
    let source = MemoryPackages::new([]).expect("empty package source should be valid");

    let error = source
        .resolve_sync(&spec)
        .expect_err("missing package should fail resolution");

    assert_eq!(error, PackageResolveError::NotFound { spec });
}

#[test]
fn resolved_package_bundles_build_render_environment_for_package_imports() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec.clone())
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file(
            "lib.typ",
            b"#let answer = [Imported from resolved bundle.]".to_vec(),
        )
        .build()
        .expect("package bundle should be valid");
    let source = MemoryPackages::new([bundle]).expect("package source should be valid");
    let environment = RenderEnvironment::builder()
        .package_bundle(
            source
                .resolve_sync(&spec)
                .expect("exact package spec should resolve before rendering"),
        )
        .build()
        .expect("render environment should accept resolved bundle");
    let workspace =
        DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\": answer\n#answer");

    let html = render_html(&workspace, &environment)
        .expect("resolved package import should render through the sandbox");

    assert!(
        html.as_str()
            .contains("<p>Imported from resolved bundle.</p>")
    );
}

#[test]
fn rendering_package_import_fails_when_bundle_was_not_resolved() {
    let workspace =
        DocumentWorkspace::from_source("#import \"@preview/missing:1.0.0\": answer\n#answer");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("empty render environment should be valid");

    let error = render_html(&workspace, &environment)
        .expect_err("unresolved package import should not render");

    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics for unresolved package import");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message().contains("@preview/missing:1.0.0")),
        "missing package diagnostics should name the unresolved exact spec: {diagnostics:?}"
    );
}

#[test]
fn package_dependency_observation_records_missing_package_requests() {
    let spec = PackageSpec::from_str("@preview/missing:1.0.0").expect("spec should parse");
    let workspace =
        DocumentWorkspace::from_source("#import \"@preview/missing:1.0.0\": answer\n#answer");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("empty render environment should be valid");

    let observation =
        observe_package_dependencies(&workspace, &environment, PackageDependencyTarget::Html)
            .expect("workspace should be valid enough for preflight");

    assert!(!observation.compile_succeeded());
    assert_eq!(observation.packages(), &[spec]);
    assert!(
        observation
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message().contains("@preview/missing:1.0.0")),
        "missing package diagnostics should name the unresolved exact spec: {:?}",
        observation.diagnostics()
    );
}

#[test]
fn package_dependency_observation_records_dynamic_package_imports() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec.clone())
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file("lib.typ", b"#let answer = [Observed dynamically.]".to_vec())
        .build()
        .expect("package bundle should be valid");
    let environment = RenderEnvironment::builder()
        .package_bundle(bundle)
        .build()
        .expect("render environment should be valid");
    let workspace = DocumentWorkspace::from_source(
        "#let package = \"@preview/example:1.2.3\"\n#import package: answer\n#answer",
    );

    let observation =
        observe_package_dependencies(&workspace, &environment, PackageDependencyTarget::Html)
            .expect("workspace should be valid enough for preflight");

    assert!(
        observation.compile_succeeded(),
        "{:?}",
        observation.diagnostics()
    );
    assert_eq!(observation.packages(), &[spec]);
}

#[test]
fn custom_world_observation_records_package_rooted_file_requests() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec.clone())
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file(
            "lib.typ",
            b"#let answer = [Observed through the world.]".to_vec(),
        )
        .build()
        .expect("package bundle should be valid");
    let environment = RenderEnvironment::builder()
        .package_bundle(bundle)
        .build()
        .expect("render environment should be valid");
    let workspace =
        DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\": answer\n#answer");
    let world =
        SandboxedWorld::for_html(workspace, environment).expect("project world should be valid");

    let observation = observe_package_dependencies_world(&world, PackageDependencyTarget::Html)
        .expect("the html Render Capability is part of default builds");

    assert!(observation.compile_succeeded());
    assert_eq!(observation.packages(), &[spec]);
}

#[test]
fn render_environment_exposes_package_bundles_by_exact_spec() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec.clone())
        .file("lib.typ", b"#let answer = 42".to_vec())
        .build()
        .expect("package bundle should be valid");
    let environment = RenderEnvironment::builder()
        .package_bundle(bundle)
        .build()
        .expect("render environment should be valid");

    let resolved_bundle = environment
        .package_bundle(&spec)
        .expect("package bundle should be available by spec");

    assert_eq!(
        resolved_bundle.file_bytes("lib.typ"),
        Some(b"#let answer = 42".as_slice())
    );
}

#[test]
fn render_environment_rejects_duplicate_package_specs() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let first = PackageBundle::builder(spec.clone())
        .file("first.typ", b"#let first = 1".to_vec())
        .build()
        .expect("first bundle should be valid");
    let second = PackageBundle::builder(spec.clone())
        .file("second.typ", b"#let second = 2".to_vec())
        .build()
        .expect("second bundle should be valid");
    let error = RenderEnvironment::builder()
        .package_bundle(first)
        .package_bundle(second)
        .build()
        .expect_err("duplicate package specs should fail validation");

    assert_eq!(error, RenderEnvironmentError::DuplicatePackage { spec });
}

#[test]
fn configured_render_dates_are_visible_to_typst_today() {
    let workspace = DocumentWorkspace::from_source("#datetime.today().year()");
    let first_date = RenderDate::from_ymd(2024, 1, 2).expect("date should be valid");
    let second_date = RenderDate::from_ymd(2025, 3, 4).expect("date should be valid");
    let first_environment = RenderEnvironment::builder()
        .render_date(first_date)
        .build()
        .expect("render environment should be valid");
    let second_environment = RenderEnvironment::builder()
        .render_date(second_date)
        .build()
        .expect("render environment should be valid");

    let first_html = render_html(&workspace, &first_environment)
        .expect("first configured Render Date should render");
    let second_html = render_html(&workspace, &second_environment)
        .expect("second configured Render Date should render");

    assert!(first_html.as_str().contains("<p>2024</p>"));
    assert!(second_html.as_str().contains("<p>2025</p>"));
    assert_ne!(first_html, second_html);
}

#[test]
fn default_render_date_keeps_today_rendering_deterministic() {
    let workspace = DocumentWorkspace::from_source("#datetime.today().year()");
    let first_environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let second_environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let first_html =
        render_html(&workspace, &first_environment).expect("default Render Date should render");
    let second_html =
        render_html(&workspace, &second_environment).expect("default Render Date should render");

    assert_eq!(
        first_environment.render_date(),
        RenderDate::from_ymd(2026, 7, 1).expect("default date should be valid")
    );
    assert!(first_html.as_str().contains("<p>2026</p>"));
    assert_eq!(first_html, second_html);
}

#[test]
fn configured_system_inputs_are_visible_to_typst() {
    let workspace = DocumentWorkspace::from_source("#sys.inputs.customer");
    let environment = RenderEnvironment::builder()
        .input("customer", "Acme")
        .build()
        .expect("render environment should be valid");

    let html = render_html(&workspace, &environment).expect("system inputs should render");

    assert!(html.as_str().contains("<p>Acme</p>"));
}

#[test]
fn system_inputs_can_be_merged_with_later_keys_winning() {
    let workspace = DocumentWorkspace::from_source("#sys.inputs.customer");
    let base_inputs = RenderEnvironment::builder()
        .input("customer", "Base")
        .build()
        .expect("base environment should be valid")
        .inputs()
        .clone();
    let environment = RenderEnvironment::builder()
        .merge_inputs(base_inputs)
        .input("customer", "Overlay")
        .build()
        .expect("merged environment should be valid");

    let html = render_html(&workspace, &environment).expect("merged system inputs should render");

    assert!(html.as_str().contains("<p>Overlay</p>"));
}

#[test]
fn explicit_font_set_makes_configured_fonts_available_to_rendering() {
    let workspace = DocumentWorkspace::from_source(
        "#set text(font: \"Libertinus Serif\")\nRendered with an explicit Font Set.",
    );
    let environment = RenderEnvironment::builder()
        .font_set(FontSet::bundled())
        .build()
        .expect("render environment should be valid");

    let pdf = render_pdf(&workspace, &environment).expect("configured font should render");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn font_set_from_font_file_bytes_makes_configured_fonts_available_to_rendering() {
    let workspace = DocumentWorkspace::from_source(
        "#set text(font: \"Libertinus Serif\")\nRendered with application-supplied font bytes.",
    );
    let environment = RenderEnvironment::builder()
        .font_set(FontSet::from_font_files(typst_assets::fonts()))
        .build()
        .expect("render environment should be valid");

    let pdf = render_pdf(&workspace, &environment).expect("configured font should render");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn font_set_from_font_file_bytes_replaces_the_bundled_default() {
    let requested_family = "Libertinus Serif";
    let font_file_without_requested_family = bundled_font_file_without_family(requested_family);
    let workspace = DocumentWorkspace::from_source(format!(
        "#set text(font: \"{requested_family}\")\nThis should not fall back to bundled fonts."
    ));
    let environment = RenderEnvironment::builder()
        .font_set(FontSet::from_font_files([
            font_file_without_requested_family,
        ]))
        .build()
        .expect("render environment should be valid");

    let error = render_pdf(&workspace, &environment)
        .expect_err("file-backed Font Set should replace bundled fonts");

    assert_unknown_font_family(error, "libertinus serif");
}

#[test]
fn bundled_plus_font_files_keeps_bundled_fonts_available() {
    let workspace = DocumentWorkspace::from_source(
        "#set text(font: \"Libertinus Serif\")\nRendered with bundled plus supplied font files.",
    );
    let environment = RenderEnvironment::builder()
        .font_set(FontSet::bundled_plus_font_files([Vec::new()]))
        .build()
        .expect("render environment should be valid");

    let pdf = render_pdf(&workspace, &environment).expect("bundled font should render");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn default_render_environment_uses_bundled_fonts() {
    let workspace = DocumentWorkspace::from_source(
        "#set text(font: \"Libertinus Serif\")\nRendered with the default Font Set.",
    );
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let pdf = render_pdf(&workspace, &environment).expect("bundled font should render");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn unconfigured_host_fonts_are_not_loaded_during_rendering() {
    let workspace = DocumentWorkspace::from_source(
        "#set text(font: \"Arial\")\nArial may exist on the host, but it is not configured.",
    );
    let environment = RenderEnvironment::builder()
        .font_set(FontSet::empty())
        .build()
        .expect("render environment should be valid");

    let error = render_pdf(&workspace, &environment)
        .expect_err("unconfigured host fonts should not render");

    assert_unknown_font_family(error, "arial");
}

#[test]
fn simple_document_workspace_renders_a_pdf_artifact() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let pdf = render_pdf(&workspace, &environment).expect("PDF render should succeed");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn sandboxed_world_renders_pdf_through_raw_world_interface() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from a Project World.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let world = SandboxedWorld::new(workspace, environment).expect("Project World should be valid");

    let pdf = render_pdf_world(&world).expect("raw world PDF render should succeed");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn html_sandboxed_world_renders_html_through_raw_world_interface() {
    let workspace =
        DocumentWorkspace::from_source("= Rendered\n\nHello from an HTML Project World.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let world = SandboxedWorld::for_html(workspace, environment)
        .expect("HTML Project World should be valid");

    let html = render_html_world(&world).expect("raw world HTML render should succeed");

    assert!(html.as_str().starts_with("<!DOCTYPE html>"));
    assert!(
        html.as_str()
            .contains("<p>Hello from an HTML Project World.</p>")
    );
}

#[test]
fn sandboxed_world_builder_enables_html_rendering() {
    let workspace = DocumentWorkspace::from_source("= Builder\n\nHTML feature enabled.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let world = SandboxedWorld::builder(workspace, environment)
        .html()
        .build()
        .expect("builder should create an HTML Project World");

    let html = render_html_world(&world).expect("builder-created world should render HTML");

    assert!(html.as_str().contains("<p>HTML feature enabled.</p>"));
}

#[test]
fn raw_html_render_reports_diagnostics_when_world_does_not_enable_html() {
    let workspace = DocumentWorkspace::from_source("= HTML Feature Required");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let world = SandboxedWorld::new(workspace, environment).expect("Project World should be valid");

    let error = render_html_world(&world)
        .expect_err("non-HTML-capable world should not render HTML artifacts");

    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics");
    };
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message()
            .contains("html export is only available")
    }));
}

#[test]
fn sandboxed_world_renders_page_images_through_raw_world_interface() {
    let workspace = DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let world = SandboxedWorld::new(workspace, environment).expect("Project World should be valid");

    let images = render_page_images_world(&world, PageImageOptions::default())
        .expect("raw world Page Image render should succeed");

    assert_eq!(images.page_count(), 2);
    assert!(
        images
            .page(0)
            .expect("first page image should exist")
            .bytes()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
}

#[test]
fn sandboxed_world_renders_selected_artifact_through_raw_world_interface() {
    let workspace =
        DocumentWorkspace::from_source("= Runtime Selected\n\nRendered as a PDF artifact.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let world = SandboxedWorld::new(workspace, environment).expect("Project World should be valid");

    let artifact = render_artifact_world(&world, RenderFormat::Pdf)
        .expect("raw world artifact render should succeed");

    let RenderArtifact::Pdf(pdf) = artifact else {
        panic!("expected PDF artifact");
    };
    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn world_overlay_replaces_workspace_file_before_delegating_to_base_world() {
    let workspace = DocumentWorkspace::builder("main.typ")
        .source_file("main.typ", "#include \"content.typ\"")
        .source_file("content.typ", "Base content.")
        .build()
        .expect("base workspace should be valid");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let base_world = SandboxedWorld::for_html(workspace, environment)
        .expect("base Project World should be valid");
    let overlay = WorldOverlay::new(base_world)
        .source_file("content.typ", "Overlay content.")
        .expect("overlay file path should be valid");

    let html = render_html_world(&overlay).expect("overlay world should render HTML");

    assert!(html.as_str().contains("<p>Overlay content.</p>"));
    assert!(!html.as_str().contains("Base content."));
}

#[test]
fn world_overlay_can_render_an_overlay_main_entrypoint() {
    let workspace = DocumentWorkspace::from_source("Base main.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let base_world = SandboxedWorld::for_html(workspace, environment)
        .expect("base Project World should be valid");
    let overlay = WorldOverlay::new(base_world)
        .source_file("preview.typ", "Overlay main.")
        .expect("overlay main file path should be valid")
        .main("preview.typ")
        .expect("overlay main path should be valid");

    let html = render_html_world(&overlay).expect("overlay main should render HTML");

    assert!(html.as_str().contains("<p>Overlay main.</p>"));
    assert!(!html.as_str().contains("Base main."));
}

#[test]
fn world_overlay_can_override_the_render_date() {
    let workspace = DocumentWorkspace::from_source("#datetime.today().year()");
    let environment = RenderEnvironment::builder()
        .render_date(RenderDate::from_ymd(2024, 1, 2).expect("date should be valid"))
        .build()
        .expect("render environment should be valid");
    let base_world = SandboxedWorld::for_html(workspace, environment)
        .expect("base Project World should be valid");
    let overlay = WorldOverlay::new(base_world)
        .render_date(RenderDate::from_ymd(2025, 3, 4).expect("date should be valid"));

    let html = render_html_world(&overlay).expect("overlay date should render HTML");

    assert!(html.as_str().contains("<p>2025</p>"));
}

#[test]
fn world_overlay_replaces_package_bundle_before_delegating_to_base_world() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let base_package = PackageBundle::builder(spec.clone())
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file("lib.typ", b"#let answer = [Base package.]".to_vec())
        .build()
        .expect("base package should be valid");
    let overlay_package = PackageBundle::builder(spec)
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file("lib.typ", b"#let answer = [Overlay package.]".to_vec())
        .build()
        .expect("overlay package should be valid");
    let workspace =
        DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\": answer\n#answer");
    let environment = RenderEnvironment::builder()
        .package_bundle(base_package)
        .build()
        .expect("render environment should be valid");
    let base_world = SandboxedWorld::for_html(workspace, environment)
        .expect("base Project World should be valid");
    let overlay = WorldOverlay::new(base_world).package_bundle(overlay_package);

    let html = render_html_world(&overlay).expect("overlay package should render HTML");

    assert!(html.as_str().contains("<p>Overlay package.</p>"));
    assert!(!html.as_str().contains("Base package."));
}

#[test]
fn world_overlay_can_replace_the_base_font_set() {
    let workspace = DocumentWorkspace::from_source(
        "#set text(font: \"Libertinus Serif\")\nThis requires the bundled Font Set.",
    );
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let base_world =
        SandboxedWorld::new(workspace, environment).expect("base Project World should be valid");
    let overlay = WorldOverlay::new(base_world).replace_font_set(FontSet::empty());

    let error = render_pdf_world(&overlay).expect_err("empty overlay Font Set should not render");

    assert_unknown_font_family(error, "libertinus serif");
}

#[test]
fn world_overlay_can_extend_the_base_font_set() {
    let workspace = DocumentWorkspace::from_source(
        "#set text(font: \"Libertinus Serif\")\nThis uses fonts added by the overlay.",
    );
    let environment = RenderEnvironment::builder()
        .font_set(FontSet::empty())
        .build()
        .expect("render environment should be valid");
    let base_world =
        SandboxedWorld::new(workspace, environment).expect("base Project World should be valid");
    let overlay = WorldOverlay::new(base_world)
        .extend_font_set(FontSet::from_font_files(typst_assets::fonts()));

    let pdf = render_pdf_world(&overlay).expect("overlay-added fonts should render");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn pdf_artifact_can_be_prepared_as_a_download_file() {
    let workspace = DocumentWorkspace::from_source("= Download\n\nSave this document.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let pdf = render_pdf(&workspace, &environment).expect("PDF render should succeed");

    let download = DownloadFile::from_pdf("document.pdf", &pdf);

    assert_eq!(download.filename(), "document.pdf");
    assert_eq!(download.media_type(), "application/pdf");
    assert!(download.bytes().starts_with(b"%PDF-"));
}

#[cfg(feature = "pack")]
#[test]
fn project_pack_round_trips_as_a_typk_download_and_renders_offline() {
    use super::{ProjectPack, ProjectPackMetadata};

    let workspace =
        DocumentWorkspace::from_source("#import \"@demo/badge:0.1.0\": badge\n\n#badge[Packed]");
    let bundle = PackageBundle::builder("@demo/badge:0.1.0".parse().expect("spec should parse"))
        .file(
            "typst.toml",
            b"[package]\nname = \"badge\"\nversion = \"0.1.0\"\nentrypoint = \"lib.typ\"".to_vec(),
        )
        .file(
            "lib.typ",
            b"#let badge(body) = box(inset: 4pt, body)".to_vec(),
        )
        .build()
        .expect("bundle should build");
    let pack = ProjectPack::builder(workspace)
        .package_bundle(bundle)
        .metadata(ProjectPackMetadata::new().with_name("Badge demo"))
        .build()
        .expect("pack should build");

    let download =
        DownloadFile::from_project_pack("project.typk", &pack).expect("pack should serialize");
    assert_eq!(download.filename(), "project.typk");
    assert_eq!(download.media_type(), "application/octet-stream");

    let imported =
        ProjectPack::from_bytes(download.bytes()).expect("downloaded pack should parse back");
    let environment = imported
        .render_environment()
        .expect("pack environment should build");
    let html =
        render_html(imported.project(), &environment).expect("imported pack should render offline");

    assert!(html.as_str().contains("Packed"));
}

#[test]
fn server_render_request_prepares_pdf_download_from_document_workspace() {
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("= Server Download\n\nSave this document."),
        Default::default(),
        DownloadFormat::Pdf,
        "document.pdf",
    );

    let download = render_download(
        request.workspace(),
        request.environment(),
        request.format(),
        request.filename(),
    )
    .expect("PDF server render request should prepare a download");

    assert_eq!(download.filename(), "document.pdf");
    assert_eq!(download.media_type(), "application/pdf");
    assert!(download.bytes().starts_with(b"%PDF-"));
}

#[test]
fn server_render_request_prepares_one_page_image_download() {
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page."),
        Default::default(),
        DownloadFormat::PageImage {
            page_index: 1,
            options: PageImageOptions::default(),
        },
        "page-2.png",
    );

    let download = render_download(
        request.workspace(),
        request.environment(),
        request.format(),
        request.filename(),
    )
    .expect("Page Image server render request should prepare a download");

    assert_eq!(download.filename(), "page-2.png");
    assert_eq!(download.media_type(), "image/png");
    assert!(download.bytes().starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn render_download_reports_out_of_range_page_indexes_as_unavailable() {
    let error = render_download(
        &DocumentWorkspace::from_source("Only one page."),
        &RenderEnvironment::default(),
        DownloadFormat::PageImage {
            page_index: 5,
            options: PageImageOptions::default(),
        },
        "page-6.png",
    )
    .expect_err("a page index past the last page should not prepare a download");

    assert_eq!(
        error,
        RenderDownloadError::Download(DownloadError::Unavailable)
    );
}

#[test]
fn server_render_request_prepares_page_image_archive_download() {
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page."),
        Default::default(),
        DownloadFormat::PageImageArchive {
            options: PageImageOptions::default(),
        },
        "pages.zip",
    );

    let download = render_download(
        request.workspace(),
        request.environment(),
        request.format(),
        request.filename(),
    )
    .expect("Page Image Archive server render request should prepare a download");
    let entries = stored_zip_entries(download.bytes());

    assert_eq!(download.filename(), "pages.zip");
    assert_eq!(download.media_type(), "application/zip");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "page-1.png");
    assert!(entries[0].1.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(entries[1].0, "page-2.png");
    assert!(entries[1].1.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn server_render_request_uses_explicit_render_environment_package_bundles() {
    let bundle =
        PackageBundle::builder("@preview/example:1.2.3".parse().expect("spec should parse"))
            .file(
                "typst.toml",
                b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                    .to_vec(),
            )
            .file("lib.typ", "#let answer = [Imported explicitly.]")
            .build()
            .expect("bundle should build");
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\": answer\n#answer"),
        RenderEnvironment::builder()
            .package_bundle(bundle)
            .build()
            .expect("environment should build"),
        DownloadFormat::Pdf,
        "document.pdf",
    );

    let download = render_download(
        request.workspace(),
        request.environment(),
        request.format(),
        request.filename(),
    )
    .expect("server render request should use explicit package bundles");

    assert_eq!(download.media_type(), "application/pdf");
    assert!(download.bytes().starts_with(b"%PDF-"));
}

#[test]
fn server_render_request_uses_explicit_render_environment_font_files() {
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source(
            "#set text(font: \"Libertinus Serif\")\nRendered with server-supplied font bytes.",
        ),
        RenderEnvironment::builder()
            .font_set(FontSet::from_font_files(typst_assets::fonts()))
            .build()
            .expect("environment should build"),
        DownloadFormat::Pdf,
        "document.pdf",
    );

    let download = render_download(
        request.workspace(),
        request.environment(),
        request.format(),
        request.filename(),
    )
    .expect("server render request should use explicit font files");

    assert_eq!(download.media_type(), "application/pdf");
    assert!(download.bytes().starts_with(b"%PDF-"));
}

#[cfg(feature = "serde")]
#[test]
fn render_environment_rejects_invalid_render_date_json() {
    let json = serde_json::json!({
        "package_bundles": [],
        "render_date": {
            "year": 2024,
            "month": 13,
            "day": 1
        }
    });

    let error = serde_json::from_value::<RenderEnvironment>(json)
        .expect_err("invalid Render Date should not deserialize");

    assert!(error.to_string().contains("invalid Render Date"), "{error}");
}

#[cfg(feature = "serde")]
#[test]
fn render_environment_accepts_valid_render_date_json() {
    let json = serde_json::json!({
        "render_date": {
            "year": 2024,
            "month": 1,
            "day": 2
        }
    });

    let environment = serde_json::from_value::<RenderEnvironment>(json)
        .expect("valid Render Date should deserialize");

    assert_eq!(
        environment.render_date(),
        RenderDate::from_ymd(2024, 1, 2).expect("date should be valid")
    );
}

#[cfg(feature = "serde")]
#[test]
fn document_workspace_json_rejects_escaping_and_duplicate_paths() {
    let escaping = serde_json::json!({
        "root_path": "main.typ",
        "files": [{"path": "../escape.typ", "bytes": []}]
    });
    let error = serde_json::from_value::<DocumentWorkspace>(escaping)
        .expect_err("escaping Project Paths should not deserialize");
    assert!(
        error.to_string().contains("invalid Project File"),
        "{error}"
    );

    let duplicate = serde_json::json!({
        "root_path": "main.typ",
        "files": [
            {"path": "main.typ", "bytes": []},
            {"path": "/main.typ", "bytes": []}
        ]
    });
    let error = serde_json::from_value::<DocumentWorkspace>(duplicate)
        .expect_err("duplicate Project Paths should not deserialize");
    assert!(
        error.to_string().contains("invalid Typst Project"),
        "{error}"
    );
}

#[cfg(feature = "serde")]
#[test]
fn server_render_request_round_trips_through_json() {
    let bundle =
        PackageBundle::builder("@preview/example:1.2.3".parse().expect("spec should parse"))
            .file("lib.typ", "#let answer = [42]")
            .build()
            .expect("bundle should build");
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("= Round Trip"),
        RenderEnvironment::builder()
            .package_bundle(bundle)
            .input("customer", "Acme")
            .build()
            .expect("environment should build"),
        DownloadFormat::Pdf,
        "document.pdf",
    );

    let json = serde_json::to_value(&request).expect("request should serialize");
    let read =
        serde_json::from_value::<ServerRenderRequest>(json).expect("request should deserialize");

    assert_eq!(read, request);
}

#[test]
fn server_render_request_does_not_fallback_to_bundled_fonts_when_font_set_is_empty() {
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source(
            "#set text(font: \"Libertinus Serif\")\nThis should not use bundled fonts.",
        ),
        RenderEnvironment::builder()
            .font_set(FontSet::empty())
            .build()
            .expect("environment should build"),
        DownloadFormat::Pdf,
        "document.pdf",
    );

    let error = render_download(
        request.workspace(),
        request.environment(),
        request.format(),
        request.filename(),
    )
    .expect_err("explicit empty Font Set input should not use bundled fonts");

    let RenderDownloadError::Render(error) = error else {
        panic!("expected render error");
    };
    assert_unknown_font_family(error, "libertinus serif");
}

#[cfg(feature = "server")]
#[test]
fn server_render_download_response_sets_pdf_download_headers() {
    let request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("= Server Download"),
        Default::default(),
        DownloadFormat::Pdf,
        "document.pdf",
    );

    let response = super::server_render_download_response(&request)
        .expect("server route response should be built for a PDF request");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        response.headers().get(axum::http::header::CONTENT_TYPE),
        Some(&axum::http::HeaderValue::from_static("application/pdf"))
    );
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_DISPOSITION),
        Some(&axum::http::HeaderValue::from_static(
            "attachment; filename=\"document.pdf\""
        ))
    );
}

#[cfg(feature = "server")]
#[test]
fn server_render_download_response_sets_page_image_and_archive_media_types() {
    let page_image_request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page."),
        Default::default(),
        DownloadFormat::PageImage {
            page_index: 0,
            options: PageImageOptions::default(),
        },
        "page-1.png",
    );
    let archive_request = ServerRenderRequest::new(
        DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page."),
        Default::default(),
        DownloadFormat::PageImageArchive {
            options: PageImageOptions::default(),
        },
        "pages.zip",
    );

    let page_image_response = super::server_render_download_response(&page_image_request)
        .expect("Page Image response should be built");
    let archive_response = super::server_render_download_response(&archive_request)
        .expect("Page Image Archive response should be built");

    assert_eq!(
        page_image_response
            .headers()
            .get(axum::http::header::CONTENT_TYPE),
        Some(&axum::http::HeaderValue::from_static("image/png"))
    );
    assert_eq!(
        archive_response
            .headers()
            .get(axum::http::header::CONTENT_TYPE),
        Some(&axum::http::HeaderValue::from_static("application/zip"))
    );
}

#[cfg(feature = "server")]
#[tokio::test]
async fn server_render_download_route_accepts_json_requests_for_download_formats() {
    let pdf_response = post_server_render_request(ServerRenderRequest::new(
        DocumentWorkspace::from_source("= Server PDF"),
        Default::default(),
        DownloadFormat::Pdf,
        "document.pdf",
    ))
    .await;
    let page_image_response = post_server_render_request(ServerRenderRequest::new(
        DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page."),
        Default::default(),
        DownloadFormat::PageImage {
            page_index: 1,
            options: PageImageOptions::default(),
        },
        "page-2.png",
    ))
    .await;
    let archive_response = post_server_render_request(ServerRenderRequest::new(
        DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page."),
        Default::default(),
        DownloadFormat::PageImageArchive {
            options: PageImageOptions::default(),
        },
        "pages.zip",
    ))
    .await;
    assert_eq!(pdf_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        pdf_response.headers().get(axum::http::header::CONTENT_TYPE),
        Some(&axum::http::HeaderValue::from_static("application/pdf"))
    );
    assert_eq!(page_image_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        page_image_response
            .headers()
            .get(axum::http::header::CONTENT_TYPE),
        Some(&axum::http::HeaderValue::from_static("image/png"))
    );
    assert_eq!(archive_response.status(), axum::http::StatusCode::OK);
    assert_eq!(
        archive_response
            .headers()
            .get(axum::http::header::CONTENT_TYPE),
        Some(&axum::http::HeaderValue::from_static("application/zip"))
    );
}

#[cfg(feature = "server")]
#[tokio::test]
async fn server_render_download_route_rejects_html_format_at_deserialization() {
    use tower::ServiceExt;

    // HTML is not a DownloadFormat variant, so a request asking for it never
    // reaches rendering: JSON deserialization rejects it at the route boundary.
    let body = serde_json::json!({
        "workspace": {"root_path": "main.typ", "files": [{"path": "main.typ", "bytes": []}]},
        "environment": {},
        "format": "Html",
        "filename": "document.html"
    });
    let response = super::server_render_download_router()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(super::SERVER_RENDER_DOWNLOAD_PATH)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&body).expect("body should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("server render route should respond");

    assert!(response.status().is_client_error(), "{}", response.status());
}

#[cfg(feature = "server")]
async fn post_server_render_request(request: ServerRenderRequest) -> axum::response::Response {
    use tower::ServiceExt;

    super::server_render_download_router()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(super::SERVER_RENDER_DOWNLOAD_PATH)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&request).expect("request should serialize"),
                ))
                .expect("request should build"),
        )
        .await
        .expect("server render route should respond")
}

#[cfg(feature = "server")]
#[test]
fn server_render_download_router_exposes_the_download_route() {
    let _router: axum::Router = super::server_render_download_router();

    assert_eq!(super::SERVER_RENDER_DOWNLOAD_PATH, "/typst/render-download");
}

#[test]
fn render_artifact_dispatches_pdf_rendering() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let artifact = render_artifact(&workspace, &environment, RenderFormat::Pdf)
        .expect("PDF artifact render should succeed");

    let RenderArtifact::Pdf(pdf) = artifact else {
        panic!("expected PDF artifact");
    };
    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn pdf_render_artifact_can_be_prepared_as_a_download_file() {
    let workspace = DocumentWorkspace::from_source("= Download\n\nSave this document.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let artifact = render_artifact(&workspace, &environment, RenderFormat::Pdf)
        .expect("PDF artifact render should succeed");

    let download = DownloadFile::from_render_artifact("document.pdf", &artifact)
        .expect("PDF artifact should be downloadable");

    assert_eq!(download.filename(), "document.pdf");
    assert_eq!(download.media_type(), "application/pdf");
    assert!(download.bytes().starts_with(b"%PDF-"));
}

#[test]
fn stale_render_artifact_state_can_be_prepared_as_a_download_file() {
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let current_workspace = DocumentWorkspace::from_source("= Download\n\nStill downloadable.");
    let broken_workspace = DocumentWorkspace::from_source("#let broken =");
    let mut state = RenderState::new();

    state.update(render_artifact(
        &current_workspace,
        &environment,
        RenderFormat::Pdf,
    ));
    state.update(render_artifact(
        &broken_workspace,
        &environment,
        RenderFormat::Pdf,
    ));

    let download = DownloadFile::from_render_artifact_state("document.pdf", &state)
        .expect("stale Render Artifact should be downloadable");

    assert_eq!(state.status(), RenderStatus::Stale);
    assert_eq!(download.filename(), "document.pdf");
    assert_eq!(download.media_type(), "application/pdf");
    assert!(download.bytes().starts_with(b"%PDF-"));
}

#[test]
fn empty_render_artifact_state_cannot_be_prepared_as_a_download_file() {
    let state = RenderState::new();

    let error = DownloadFile::from_render_artifact_state("document.pdf", &state)
        .expect_err("empty Render Artifact state should not be downloadable");

    assert_eq!(state.status(), RenderStatus::Empty);
    assert_eq!(error, DownloadError::Unavailable);
}

#[test]
fn html_render_artifact_state_cannot_be_prepared_as_a_download_file() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let mut state = RenderState::new();

    state.update(render_artifact(
        &workspace,
        &environment,
        RenderFormat::Html,
    ));

    let error = DownloadFile::from_render_artifact_state("document.html", &state)
        .expect_err("HTML Render Artifact state should not be downloadable");

    assert_eq!(state.status(), RenderStatus::Current);
    assert_eq!(error, DownloadError::UnsupportedArtifact);
}

#[test]
fn root_file_diagnostics_expose_workspace_path_and_source_range() {
    let workspace = DocumentWorkspace::from_source("= Title\n\n#let broken =");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let error = render_pdf(&workspace, &environment)
        .expect_err("invalid Typst source should fail with diagnostics");

    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics");
    };

    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message().contains("expected expression"))
        .expect("expected expression diagnostic should be reported");
    let range = diagnostic
        .source_range()
        .expect("root file diagnostic should expose its source range");

    assert_eq!(
        diagnostic
            .workspace_path()
            .map(|path| path.get_without_slash()),
        Some("main.typ")
    );
    assert_eq!(range.start_line(), 2);
    assert_eq!(range.end_line(), 2);
}

#[test]
fn included_file_diagnostics_expose_included_workspace_path() {
    let workspace = DocumentWorkspace::builder("main.typ")
        .source_file("main.typ", "= Main\n\n#include \"chapters/intro.typ\"")
        .source_file("chapters/intro.typ", "Included\n#let broken =")
        .build()
        .expect("workspace should be valid");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let error = render_pdf(&workspace, &environment)
        .expect_err("invalid included Typst source should fail with diagnostics");

    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics");
    };
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message().contains("expected expression"))
        .expect("expected expression diagnostic should be reported");
    let range = diagnostic
        .source_range()
        .expect("included file diagnostic should expose its source range");

    assert_eq!(
        diagnostic
            .workspace_path()
            .map(|path| path.get_without_slash()),
        Some("chapters/intro.typ")
    );
    assert_eq!(range.start_line(), 1);
    assert_eq!(range.end_line(), 1);
}

#[test]
fn package_file_diagnostics_expose_source_identity_without_workspace_path() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec)
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file("lib.typ", b"#let broken =".to_vec())
        .build()
        .expect("package bundle should be valid");
    let workspace = DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\"");
    let environment = RenderEnvironment::builder()
        .package_bundle(bundle)
        .build()
        .expect("render environment should be valid");
    let error = render_pdf(&workspace, &environment)
        .expect_err("invalid package source should fail with diagnostics");

    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics");
    };
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message().contains("expected expression"))
        .expect("package diagnostic should be reported");
    let source = diagnostic
        .source_identity()
        .expect("package diagnostic should expose a source identity");

    assert!(diagnostic.workspace_path().is_none());
    assert_eq!(source.package(), Some("@preview/example:1.2.3"));
    assert_eq!(source.path(), "lib.typ");
}

#[cfg(feature = "serde")]
#[test]
fn render_diagnostics_serialize_for_dioxus_flows() {
    let workspace = DocumentWorkspace::from_source("= Title\n\n#let broken =");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let error = render_pdf(&workspace, &environment)
        .expect_err("invalid Typst source should fail with diagnostics");

    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics");
    };
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message().contains("expected expression"))
        .expect("expected expression diagnostic should be reported");
    let value = serde_json::to_value(diagnostic)
        .expect("render diagnostic should serialize under the serde feature");

    assert_eq!(value["message"], "expected expression");
    assert_eq!(value["workspace_path"], "main.typ");
    assert_eq!(value["source_range"]["start_line"], 2);
}

#[test]
fn html_export_diagnostics_expose_workspace_path_and_source_range() {
    let workspace = DocumentWorkspace::from_source("#html.elem(\"script\")[</script>]");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let error = render_html(&workspace, &environment)
        .expect_err("invalid HTML export should fail with diagnostics");

    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics");
    };
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .message()
                .contains("HTML raw text element cannot contain its own closing tag")
        })
        .expect("HTML export diagnostic should be reported");
    let range = diagnostic
        .source_range()
        .expect("HTML export diagnostic should expose its source range");

    assert_eq!(
        diagnostic
            .workspace_path()
            .map(|path| path.get_without_slash()),
        Some("main.typ")
    );
    assert_eq!(range.start_line(), 0);
    assert_eq!(range.end_line(), 0);
}

#[test]
fn rendered_source_can_include_another_workspace_file() {
    let workspace = DocumentWorkspace::builder("main.typ")
        .source_file("main.typ", "= Main\n\n#include \"chapters/intro.typ\"")
        .source_file("chapters/intro.typ", "Included from the workspace.")
        .build()
        .expect("workspace should be valid");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let pdf = render_pdf(&workspace, &environment).expect("PDF render should succeed");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn rendered_source_can_import_a_package_bundle() {
    let spec = PackageSpec::from_str("@preview/example:1.2.3").expect("spec should parse");
    let bundle = PackageBundle::builder(spec)
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file(
            "lib.typ",
            "#let answer = [Imported from package.]".as_bytes().to_vec(),
        )
        .build()
        .expect("package bundle should be valid");
    let workspace =
        DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\": answer\n#answer");
    let environment = RenderEnvironment::builder()
        .package_bundle(bundle)
        .build()
        .expect("render environment should be valid");

    let pdf = render_pdf(&workspace, &environment).expect("PDF render should succeed");

    assert!(pdf.bytes().starts_with(b"%PDF-"));
}

#[test]
fn one_page_document_workspace_renders_one_png_page_image() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let images = render_page_images(&workspace, &environment, PageImageOptions::default())
        .expect("Page Image render should succeed");

    assert_eq!(images.page_count(), 1);
    assert!(
        images
            .page(0)
            .expect("first page image should exist")
            .bytes()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
}

#[test]
fn page_image_can_be_prepared_as_a_download_file() {
    let workspace = DocumentWorkspace::from_source("= Download\n\nSave this page.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let images = render_page_images(&workspace, &environment, PageImageOptions::default())
        .expect("Page Image render should succeed");
    let page = images.page(0).expect("first page image should exist");

    let download = DownloadFile::from_page_image("page-1.png", page);

    assert_eq!(download.filename(), "page-1.png");
    assert_eq!(download.media_type(), "image/png");
    assert!(download.bytes().starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn page_images_artifact_can_be_prepared_as_an_archive_download_file() {
    let workspace = DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let images = render_page_images(&workspace, &environment, PageImageOptions::default())
        .expect("Page Image render should succeed");

    let download = DownloadFile::from_page_images_archive("pages.zip", &images);
    let archive = stored_zip_archive(download.bytes());
    let first_page = images.page(0).expect("first Page Image should exist");
    let second_page = images.page(1).expect("second Page Image should exist");

    assert_eq!(download.filename(), "pages.zip");
    assert_eq!(download.media_type(), "application/zip");
    assert_eq!(archive.entries.len(), 2);
    assert_eq!(archive.entries[0].name, "page-1.png");
    assert!(archive.entries[0].data.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(archive.entries[0].data.as_slice(), first_page.bytes());
    assert_eq!(archive.entries[1].name, "page-2.png");
    assert!(archive.entries[1].data.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(archive.entries[1].data.as_slice(), second_page.bytes());
    assert_eq!(archive.entries[0].local_header_offset, 0);
    assert_eq!(
        archive.entries[0].data_end,
        archive.entries[1].local_header_offset
    );
    assert_eq!(
        archive.entries[1].data_end,
        archive.central_directory_offset
    );
    assert_eq!(
        archive.central_directory_offset + archive.central_directory_size,
        archive.eocd_offset
    );
}

#[test]
fn multi_page_document_workspace_renders_one_png_page_image_per_page() {
    let workspace = DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let images = render_page_images(&workspace, &environment, PageImageOptions::default())
        .expect("Page Image render should succeed");

    assert_eq!(images.page_count(), 2);
    assert!(
        images
            .page(0)
            .expect("first page image should exist")
            .bytes()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    assert!(
        images
            .page(1)
            .expect("second page image should exist")
            .bytes()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
}

#[test]
fn page_image_options_scale_rendered_image_dimensions() {
    let workspace = DocumentWorkspace::from_source(
        "#set page(width: 100pt, height: 50pt, margin: 0pt)\nScaled page.",
    );
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let one_pixel_per_pt = render_page_images(&workspace, &environment, PageImageOptions::new(1.0))
        .expect("Page Image render should succeed");
    let two_pixels_per_pt =
        render_page_images(&workspace, &environment, PageImageOptions::new(2.0))
            .expect("Page Image render should succeed");

    let one_pixel_per_pt_page = one_pixel_per_pt
        .page(0)
        .expect("first scaled page image should exist");
    let two_pixels_per_pt_page = two_pixels_per_pt
        .page(0)
        .expect("second scaled page image should exist");

    assert_eq!(one_pixel_per_pt_page.width(), 100);
    assert_eq!(one_pixel_per_pt_page.height(), 50);
    assert_eq!(two_pixels_per_pt_page.width(), 200);
    assert_eq!(two_pixels_per_pt_page.height(), 100);
    assert_ne!(
        one_pixel_per_pt_page.bytes(),
        two_pixels_per_pt_page.bytes()
    );
}

#[test]
fn render_artifact_dispatches_page_images_rendering_with_options() {
    let workspace = DocumentWorkspace::from_source(
        "#set page(width: 100pt, height: 50pt, margin: 0pt)\nScaled page.",
    );
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let artifact = render_artifact(
        &workspace,
        &environment,
        RenderFormat::PageImages(PageImageOptions::new(1.0)),
    )
    .expect("Page Images artifact render should succeed");

    let RenderArtifact::PageImages(images) = artifact else {
        panic!("expected Page Images artifact");
    };
    let page = images.page(0).expect("first page image should exist");
    assert_eq!(images.page_count(), 1);
    assert_eq!(page.width(), 100);
    assert_eq!(page.height(), 50);
    assert!(page.bytes().starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn page_images_render_artifact_can_be_prepared_as_an_archive_download_file() {
    let workspace = DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let artifact = render_artifact(
        &workspace,
        &environment,
        RenderFormat::PageImages(PageImageOptions::default()),
    )
    .expect("Page Images artifact render should succeed");

    let download = DownloadFile::from_render_artifact("pages.zip", &artifact)
        .expect("Page Images artifact should be downloadable as an archive");
    let entries = stored_zip_entries(download.bytes());

    assert_eq!(download.filename(), "pages.zip");
    assert_eq!(download.media_type(), "application/zip");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "page-1.png");
    assert!(entries[0].1.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(entries[1].0, "page-2.png");
    assert!(entries[1].1.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn simple_document_workspace_renders_an_html_artifact() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let html = render_html(&workspace, &environment).expect("HTML render should succeed");

    assert!(html.as_str().starts_with("<!DOCTYPE html>\n<html"));
    assert!(html.as_str().contains("<h2>Rendered</h2>"));
    assert!(
        html.as_str().contains("<p>Hello from Typst.</p>"),
        "{}",
        html.as_str()
    );
}

#[test]
fn render_artifact_dispatches_html_rendering() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let artifact = render_artifact(&workspace, &environment, RenderFormat::Html)
        .expect("HTML artifact render should succeed");

    let RenderArtifact::Html(html) = artifact else {
        panic!("expected HTML artifact");
    };
    assert!(html.as_str().starts_with("<!DOCTYPE html>\n<html"));
    assert!(html.as_str().contains("<p>Hello from Typst.</p>"));
}

#[test]
fn html_render_artifact_cannot_be_prepared_as_a_download_file() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let artifact = render_artifact(&workspace, &environment, RenderFormat::Html)
        .expect("HTML artifact render should succeed");

    let error = DownloadFile::from_render_artifact("document.html", &artifact)
        .expect_err("HTML artifacts should not be downloadable");

    assert_eq!(error, DownloadError::UnsupportedArtifact);
}

#[test]
fn headless_render_action_updates_current_render_state() {
    let workspace = DocumentWorkspace::from_source("= Rendered\n\nHello from Typst.");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let mut renderer = HeadlessRender::new();

    renderer.render(&workspace, &environment, RenderFormat::Html);

    let state = renderer.state();
    assert_eq!(state.status(), RenderStatus::Current);
    let Some(RenderArtifact::Html(html)) = state.artifact() else {
        panic!("expected current HTML artifact");
    };
    assert!(html.as_str().contains("<p>Hello from Typst.</p>"));
    assert!(state.error().is_none());
}

#[test]
fn headless_render_action_retains_stale_artifact_after_error() {
    let current_workspace = DocumentWorkspace::from_source("= Current\n\nStill visible.");
    let broken_workspace = DocumentWorkspace::from_source("#let broken =");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let mut renderer = HeadlessRender::new();

    renderer.render(&current_workspace, &environment, RenderFormat::Html);
    renderer.render(&broken_workspace, &environment, RenderFormat::Html);

    let state = renderer.state();
    assert_eq!(state.status(), RenderStatus::Stale);
    let Some(RenderArtifact::Html(html)) = state.artifact() else {
        panic!("expected stale HTML artifact");
    };
    assert!(html.as_str().contains("<p>Still visible.</p>"));
    let Some(RenderError::Diagnostics(diagnostics)) = state.error() else {
        panic!("expected Typst diagnostics");
    };
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message().contains("expected expression"))
        .expect("expected expression diagnostic should be reported");

    assert_eq!(
        diagnostic
            .workspace_path()
            .map(|path| path.get_without_slash()),
        Some("main.typ")
    );
    assert!(diagnostic.source_range().is_some());
}

#[test]
fn headless_render_action_records_failed_state_without_artifact() {
    let broken_workspace = DocumentWorkspace::from_source("#let broken =");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let mut renderer = HeadlessRender::new();

    renderer.render(&broken_workspace, &environment, RenderFormat::Html);

    let state = renderer.state();
    assert_eq!(state.status(), RenderStatus::Failed);
    assert!(state.artifact().is_none());
    assert!(matches!(state.error(), Some(RenderError::Diagnostics(_))));
}

#[test]
fn headless_render_action_does_not_keep_stale_artifact_when_format_changes() {
    let current_workspace = DocumentWorkspace::from_source("= Current\n\nPDF artifact.");
    let broken_workspace = DocumentWorkspace::from_source("#let broken =");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let mut renderer = HeadlessRender::new();

    renderer.render(&current_workspace, &environment, RenderFormat::Pdf);
    renderer.render(&broken_workspace, &environment, RenderFormat::Html);

    let state = renderer.state();
    assert_eq!(state.status(), RenderStatus::Failed);
    assert!(state.artifact().is_none());
    assert!(matches!(state.error(), Some(RenderError::Diagnostics(_))));
}

#[test]
fn headless_render_keeps_stale_artifact_after_failed_render() {
    let current_workspace = DocumentWorkspace::from_source("= Current\n\nStill visible.");
    let broken_workspace = DocumentWorkspace::from_source("#let broken =");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let mut renderer = HeadlessRender::new();

    renderer.render(&current_workspace, &environment, RenderFormat::Html);
    renderer.render(&broken_workspace, &environment, RenderFormat::Html);

    let state = renderer.state();
    assert_eq!(state.status(), RenderStatus::Stale);
    let Some(RenderArtifact::Html(html)) = state.artifact() else {
        panic!("expected stale HTML artifact");
    };
    assert!(html.as_str().contains("<p>Still visible.</p>"));
    assert!(matches!(state.error(), Some(RenderError::Diagnostics(_))));
}

#[test]
fn rendered_html_can_include_another_workspace_file() {
    let workspace = DocumentWorkspace::builder("main.typ")
        .source_file("main.typ", "= Main\n\n#include \"chapters/intro.typ\"")
        .source_file("chapters/intro.typ", "Included from the workspace.")
        .build()
        .expect("workspace should be valid");
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");

    let html = render_html(&workspace, &environment).expect("HTML render should succeed");

    assert!(
        html.as_str()
            .contains("<p>Included from the workspace.</p>")
    );
}

#[test]
fn render_state_keeps_stale_artifact_when_next_render_fails() {
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let current_workspace = DocumentWorkspace::from_source("= Current\n\nStill visible.");
    let broken_workspace = DocumentWorkspace::from_source("#let broken =");
    let mut state = RenderState::new();

    state.update(render_html(&current_workspace, &environment));
    state.update(render_html(&broken_workspace, &environment));

    assert_eq!(state.status(), RenderStatus::Stale);
    assert!(
        state
            .artifact()
            .expect("stale HTML artifact should remain available")
            .as_str()
            .contains("<p>Still visible.</p>")
    );

    let RenderError::Diagnostics(diagnostics) = state
        .error()
        .expect("failed render should remain available")
    else {
        panic!("expected Typst diagnostics");
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message().contains("expected expression"))
    );
}

#[test]
fn render_state_clears_error_when_render_recovers() {
    let environment = RenderEnvironment::builder()
        .build()
        .expect("render environment should be valid");
    let broken_workspace = DocumentWorkspace::from_source("#let broken =");
    let recovered_workspace = DocumentWorkspace::from_source("= Recovered\n\nVisible again.");
    let mut state = RenderState::new();

    state.update(render_html(&broken_workspace, &environment));

    assert_eq!(state.status(), RenderStatus::Failed);
    assert!(state.artifact().is_none());
    assert!(state.error().is_some());

    state.update(render_html(&recovered_workspace, &environment));

    assert_eq!(state.status(), RenderStatus::Current);
    assert!(state.error().is_none());
    assert!(
        state
            .artifact()
            .expect("current HTML artifact should be available")
            .as_str()
            .contains("<p>Visible again.</p>")
    );
}

#[test]
fn test_zip_crc32_uses_standard_check_value() {
    assert_eq!(test_zip_crc32(b"123456789"), 0xcbf4_3926);
}

#[cfg(feature = "dioxus")]
mod dioxus_provider_tests {
    use super::*;
    use crate::{
        Typst, TypstInput, TypstProvider, TypstProviderDefaults, TypstView, use_typst_defaults,
        use_typst_render,
    };
    use dioxus::prelude::*;
    use std::cell::RefCell;

    thread_local! {
        static PROVIDED_DEFAULTS: RefCell<Option<TypstProviderDefaults>> = const { RefCell::new(None) };
        static HOOK_RENDERER: RefCell<Option<Signal<HeadlessRender>>> = const { RefCell::new(None) };
    }

    fn defaults_consumer() -> Element {
        let defaults = use_typst_defaults();

        use_hook(move || {
            PROVIDED_DEFAULTS.with(|cell| {
                *cell.borrow_mut() = Some(defaults.clone());
            });
        });

        rsx! {}
    }

    fn provider_app() -> Element {
        let render_date = RenderDate::from_ymd(2024, 2, 3).expect("date should be valid");
        let environment = RenderEnvironment::builder()
            .render_date(render_date)
            .build()
            .expect("render environment should be valid");
        let defaults = TypstProviderDefaults::new(environment);

        rsx! {
            TypstProvider { defaults,
                defaults_consumer {}
            }
        }
    }

    fn default_app() -> Element {
        defaults_consumer()
    }

    fn render_hook_app() -> Element {
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

        let mut dom = VirtualDom::new(render_hook_app);
        dom.rebuild_in_place();
        let renderer = HOOK_RENDERER.with(|cell| {
            cell.borrow_mut()
                .take()
                .expect("hook should expose a HeadlessRender signal")
        });

        (dom, renderer)
    }

    #[test]
    fn typst_defaults_are_available_without_provider() {
        PROVIDED_DEFAULTS.with(|cell| {
            *cell.borrow_mut() = None;
        });

        let mut dom = VirtualDom::new(default_app);
        dom.rebuild_in_place();
        let defaults = PROVIDED_DEFAULTS.with(|cell| {
            cell.borrow_mut()
                .take()
                .expect("consumer should receive default Typst settings")
        });

        assert_eq!(
            defaults.environment().render_date(),
            RenderDate::from_ymd(2026, 7, 1).expect("date should be valid")
        );
        assert!(defaults.package_source().is_none());
    }

    #[test]
    fn typst_provider_supplies_defaults_to_descendants() {
        PROVIDED_DEFAULTS.with(|cell| {
            *cell.borrow_mut() = None;
        });

        let mut dom = VirtualDom::new(provider_app);
        dom.rebuild_in_place();
        let defaults = PROVIDED_DEFAULTS.with(|cell| {
            cell.borrow_mut()
                .take()
                .expect("descendant should receive Typst Provider defaults")
        });

        assert_eq!(
            defaults.environment().render_date(),
            RenderDate::from_ymd(2024, 2, 3).expect("date should be valid")
        );
    }

    #[test]
    fn dioxus_render_hook_renders_complete_world() {
        let (_dom, mut renderer) = render_signal_from_hook();
        let workspace = DocumentWorkspace::from_source("= Hook World\n\nRendered through a hook.");
        let environment = RenderEnvironment::builder()
            .build()
            .expect("render environment should be valid");
        let world = SandboxedWorld::for_html(workspace, environment)
            .expect("Project World should be valid");

        renderer.write().render_world(&world, RenderFormat::Html);

        let renderer = renderer.read();
        let state = renderer.state();
        assert_eq!(state.status(), RenderStatus::Current);
        let Some(RenderArtifact::Html(html)) = state.artifact() else {
            panic!("expected HTML artifact");
        };
        assert!(html.as_str().contains("Hook World"));
    }

    #[test]
    fn typst_component_renders_explicit_html_view() {
        let html = dioxus_ssr::render_element(rsx! {
            Typst {
                input: TypstInput::source("= Component View\n\nRendered as semantic HTML."),
                view: TypstView::Html,
            }
        });

        assert!(html.contains("Component View"), "{html}");
        assert!(html.contains("Rendered as semantic HTML."), "{html}");
    }

    #[test]
    fn typst_component_renders_explicit_pdf_frame_view() {
        let html = dioxus_ssr::render_element(rsx! {
            Typst {
                input: TypstInput::source("= Component PDF"),
                view: TypstView::PdfFrame,
            }
        });

        assert!(html.contains("<iframe"), "{html}");
        assert!(html.contains("data:application/pdf;base64,"), "{html}");
    }

    #[test]
    fn typst_component_renders_explicit_page_images_view() {
        let html = dioxus_ssr::render_element(rsx! {
            Typst {
                input: TypstInput::source("First page.\n#pagebreak()\nSecond page."),
                view: TypstView::PageImages(PageImageOptions::default()),
            }
        });

        assert!(html.contains("<img"), "{html}");
        assert!(html.contains("Typst page 1"), "{html}");
        assert!(html.contains("Typst page 2"), "{html}");
        assert!(html.contains("data:image/png;base64,"), "{html}");
    }

    #[test]
    fn typst_component_exposes_render_status_and_error_states() {
        let html = dioxus_ssr::render_element(rsx! {
            Typst {
                input: TypstInput::source("= Fine"),
                view: TypstView::Html,
            }
        });
        assert!(html.contains("data-render-status=\"current\""), "{html}");

        let html = dioxus_ssr::render_element(rsx! {
            Typst {
                input: TypstInput::source("#import \"@preview/missing:1.0.0\": x"),
                view: TypstView::Html,
            }
        });
        assert!(html.contains("data-render-status=\"failed\""), "{html}");
        assert!(html.contains("typst-error"), "{html}");
    }
}

#[cfg(feature = "dioxus")]
mod render_session_tests {
    use super::*;
    use crate::{
        RenderSession, RenderSessionOptions, SharedPackageSource, TypstInput, TypstView,
        WorldPreparationPhase, use_render_session,
    };
    use dioxus::dioxus_core::NoOpMutations;
    use dioxus::prelude::*;
    use std::cell::RefCell;
    use std::time::Duration;

    thread_local! {
        static SESSION: RefCell<Option<RenderSession>> = const { RefCell::new(None) };
        static SOURCE_SIGNAL: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
        static PACKAGE_SOURCE: RefCell<Option<SharedPackageSource>> = const { RefCell::new(None) };
    }

    fn session_app() -> Element {
        let package_source = PACKAGE_SOURCE.with(|cell| cell.borrow().clone());
        let source = use_signal(|| String::from("= First"));
        use_hook(move || {
            SOURCE_SIGNAL.with(|cell| *cell.borrow_mut() = Some(source));
        });

        let mut options = RenderSessionOptions::new();
        if let Some(package_source) = package_source {
            options = options.package_source(package_source);
        }
        let session = use_render_session(
            TypstInput::source(source.read().clone()),
            TypstView::Html,
            options,
        );
        SESSION.with(|cell| *cell.borrow_mut() = Some(session));

        rsx! {}
    }

    fn mount(source: Option<SharedPackageSource>) -> (VirtualDom, RenderSession) {
        PACKAGE_SOURCE.with(|cell| *cell.borrow_mut() = source);
        SESSION.with(|cell| *cell.borrow_mut() = None);
        SOURCE_SIGNAL.with(|cell| *cell.borrow_mut() = None);

        let mut dom = VirtualDom::new(session_app);
        dom.rebuild_in_place();
        let session = SESSION.with(|cell| cell.borrow().expect("session should be exposed"));

        (dom, session)
    }

    fn session_html(session: &RenderSession) -> Option<String> {
        let renderer = session.state();
        let renderer = renderer.read();
        match renderer.state().artifact() {
            Some(RenderArtifact::Html(html)) => Some(html.as_str().to_owned()),
            _ => None,
        }
    }

    async fn drive_until(dom: &mut VirtualDom, mut done: impl FnMut() -> bool) {
        for _ in 0..64 {
            if done() {
                return;
            }
            let step = tokio::time::timeout(Duration::from_secs(60), dom.wait_for_work());
            if step.await.is_err() {
                break;
            }
            dom.render_immediate(&mut NoOpMutations);
        }
        assert!(done(), "virtual dom work did not reach the expected state");
    }

    fn set_source(text: &str) {
        let mut source =
            SOURCE_SIGNAL.with(|cell| cell.borrow().expect("source signal should be exposed"));
        source.set(text.to_owned());
    }

    #[test]
    fn session_renders_immediately_on_mount() {
        let (_dom, session) = mount(None);

        let renderer = session.state();
        let renderer = renderer.read();
        assert_eq!(renderer.state().status(), RenderStatus::Current);
        drop(renderer);
        assert!(
            session_html(&session)
                .expect("mount should render synchronously")
                .contains("First")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn session_rerenders_when_the_input_changes() {
        let (mut dom, session) = mount(None);

        // The Render Policy is the caller's signal wiring: setting the input signal is
        // the render trigger, whether the app sets it per keystroke, debounced, or on
        // an explicit commit action.
        set_source("= Second");
        drive_until(&mut dom, || {
            session_html(&session).is_some_and(|html| html.contains("Second"))
        })
        .await;

        let renderer = session.state();
        let renderer = renderer.read();
        assert_eq!(renderer.state().status(), RenderStatus::Current);
    }

    #[tokio::test(start_paused = true)]
    async fn session_retains_a_stale_artifact_when_new_input_fails() {
        let (mut dom, session) = mount(None);

        set_source("#let broken =");
        drive_until(&mut dom, || {
            let renderer = session.state();
            let renderer = renderer.read();
            renderer.state().status() == RenderStatus::Stale
        })
        .await;

        assert!(
            session_html(&session)
                .expect("stale artifact should remain visible")
                .contains("First")
        );
        let renderer = session.state();
        let renderer = renderer.read();
        assert!(renderer.state().error().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn session_rerenders_when_package_preparation_completes() {
        let bundle = PackageBundle::builder(
            "@preview/example:1.0.0".parse().expect("spec should parse"),
        )
        .file(
            "typst.toml",
            b"[package]\nname = \"example\"\nversion = \"1.0.0\"\nentrypoint = \"lib.typ\"\n"
                .to_vec(),
        )
        .file("lib.typ", "#let answer = [Prepared package content.]")
        .build()
        .expect("bundle should build");
        let source = SharedPackageSource::new(
            MemoryPackages::new([bundle]).expect("package source should be valid"),
        );

        let (mut dom, session) = mount(Some(source));

        set_source("#import \"@preview/example:1.0.0\": answer\n#answer");

        // Preparation resolves the package and finishes with an enriched environment,
        // which re-renders the session without any further input change.
        drive_until(&mut dom, || {
            session.preparation().read().phase() == WorldPreparationPhase::Ready
                && session_html(&session)
                    .is_some_and(|html| html.contains("Prepared package content."))
        })
        .await;

        let renderer = session.state();
        let renderer = renderer.read();
        assert_eq!(renderer.state().status(), RenderStatus::Current);
    }
}

fn assert_unknown_font_family(error: RenderError, family: &str) {
    let RenderError::Diagnostics(diagnostics) = error else {
        panic!("expected Typst diagnostics");
    };

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message() == format!("unknown font family: {family}"))
    );
}

fn bundled_font_file_without_family(family: &str) -> Vec<u8> {
    typst_assets::fonts()
        .find(|bytes| FontInfo::iter(bytes).all(|info| info.family != family))
        .expect("bundled font fixture should contain another family")
        .to_vec()
}

struct StoredZipArchive {
    entries: Vec<StoredZipEntry>,
    central_directory_offset: usize,
    central_directory_size: usize,
    eocd_offset: usize,
}

struct StoredZipEntry {
    name: String,
    data: Vec<u8>,
    local_header_offset: usize,
    data_end: usize,
}

struct LocalZipEntry {
    name: String,
    data: Vec<u8>,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    data_end: usize,
}

fn stored_zip_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    stored_zip_archive(bytes)
        .entries
        .into_iter()
        .map(|entry| (entry.name, entry.data))
        .collect()
}

fn stored_zip_archive(bytes: &[u8]) -> StoredZipArchive {
    let eocd_offset = bytes
        .windows(4)
        .rposition(|signature| signature == b"PK\x05\x06")
        .expect("zip archive should contain an end of central directory record");
    assert_eq!(
        bytes.get(eocd_offset..eocd_offset + 4),
        Some(&b"PK\x05\x06"[..])
    );
    assert_eq!(read_u16_le(bytes, eocd_offset + 4), 0);
    assert_eq!(read_u16_le(bytes, eocd_offset + 6), 0);
    let disk_entry_count = read_u16_le(bytes, eocd_offset + 8);
    let total_entry_count = read_u16_le(bytes, eocd_offset + 10);
    let central_directory_size = read_u32_le(bytes, eocd_offset + 12) as usize;
    let central_directory_offset = read_u32_le(bytes, eocd_offset + 16) as usize;
    let comment_length = read_u16_le(bytes, eocd_offset + 20) as usize;

    assert_eq!(disk_entry_count, total_entry_count);
    assert_eq!(comment_length, 0);
    assert_eq!(eocd_offset + 22 + comment_length, bytes.len());
    assert_eq!(
        central_directory_offset + central_directory_size,
        eocd_offset
    );

    let mut entries = Vec::new();
    let mut central_offset = central_directory_offset;

    for _ in 0..total_entry_count {
        assert_eq!(
            bytes.get(central_offset..central_offset + 4),
            Some(&b"PK\x01\x02"[..])
        );
        assert_eq!(read_u16_le(bytes, central_offset + 4), 20);
        assert_eq!(read_u16_le(bytes, central_offset + 6), 20);
        assert_eq!(read_u16_le(bytes, central_offset + 8), 0);
        assert_eq!(read_u16_le(bytes, central_offset + 10), 0);
        assert_eq!(read_u16_le(bytes, central_offset + 12), 0);
        assert_eq!(read_u16_le(bytes, central_offset + 14), 0);
        let crc32 = read_u32_le(bytes, central_offset + 16);
        let compressed_size = read_u32_le(bytes, central_offset + 20);
        let uncompressed_size = read_u32_le(bytes, central_offset + 24);
        let name_length = read_u16_le(bytes, central_offset + 28) as usize;
        let extra_length = read_u16_le(bytes, central_offset + 30) as usize;
        let comment_length = read_u16_le(bytes, central_offset + 32) as usize;
        let disk_start = read_u16_le(bytes, central_offset + 34);
        let internal_attributes = read_u16_le(bytes, central_offset + 36);
        let external_attributes = read_u32_le(bytes, central_offset + 38);
        let local_header_offset = read_u32_le(bytes, central_offset + 42) as usize;
        let name_start = central_offset + 46;
        let name_end = name_start + name_length;
        let extra_end = name_end + extra_length;
        let comment_end = extra_end + comment_length;

        assert_eq!(extra_length, 0);
        assert_eq!(comment_length, 0);
        assert_eq!(disk_start, 0);
        assert_eq!(internal_attributes, 0);
        assert_eq!(external_attributes, 0);

        let central_name = std::str::from_utf8(&bytes[name_start..name_end])
            .expect("zip central directory entry name should be valid UTF-8")
            .to_owned();
        let local_entry = stored_zip_local_entry(bytes, local_header_offset);

        assert_eq!(central_name, local_entry.name);
        assert_eq!(crc32, local_entry.crc32);
        assert_eq!(compressed_size, local_entry.compressed_size);
        assert_eq!(uncompressed_size, local_entry.uncompressed_size);
        assert_eq!(compressed_size, uncompressed_size);
        assert_eq!(crc32, test_zip_crc32(&local_entry.data));
        assert!(local_entry.data_end <= central_directory_offset);

        entries.push(StoredZipEntry {
            name: central_name,
            data: local_entry.data,
            local_header_offset,
            data_end: local_entry.data_end,
        });
        central_offset = comment_end;
    }

    assert_eq!(central_offset, eocd_offset);

    StoredZipArchive {
        entries,
        central_directory_offset,
        central_directory_size,
        eocd_offset,
    }
}

fn stored_zip_local_entry(bytes: &[u8], offset: usize) -> LocalZipEntry {
    assert_eq!(bytes.get(offset..offset + 4), Some(&b"PK\x03\x04"[..]));
    assert_eq!(read_u16_le(bytes, offset + 4), 20);
    assert_eq!(read_u16_le(bytes, offset + 6), 0);
    assert_eq!(read_u16_le(bytes, offset + 8), 0);
    assert_eq!(read_u16_le(bytes, offset + 10), 0);
    assert_eq!(read_u16_le(bytes, offset + 12), 0);
    let crc32 = read_u32_le(bytes, offset + 14);
    let compressed_size = read_u32_le(bytes, offset + 18);
    let uncompressed_size = read_u32_le(bytes, offset + 22);
    let name_length = read_u16_le(bytes, offset + 26) as usize;
    let extra_length = read_u16_le(bytes, offset + 28) as usize;
    let name_start = offset + 30;
    let name_end = name_start + name_length;
    let data_start = name_end + extra_length;
    let data_end = data_start + compressed_size as usize;

    assert_eq!(extra_length, 0);
    assert_eq!(compressed_size, uncompressed_size);

    let name = std::str::from_utf8(&bytes[name_start..name_end])
        .expect("zip local file entry name should be valid UTF-8")
        .to_owned();
    let data = bytes[data_start..data_end].to_vec();

    LocalZipEntry {
        name,
        data,
        crc32,
        compressed_size,
        uncompressed_size,
        data_end,
    }
}

fn test_zip_crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;

    for byte in bytes {
        crc ^= u32::from(byte.reverse_bits()) << 24;

        for _ in 0..8 {
            if crc & 0x8000_0000 == 0 {
                crc <<= 1;
            } else {
                crc = (crc << 1) ^ 0x04c1_1db7;
            }
        }
    }

    !crc.reverse_bits()
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
