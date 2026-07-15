# diotypst

[![crates.io](https://img.shields.io/crates/v/diotypst?style=flat-square)](https://crates.io/crates/diotypst)
[![docs.rs](https://img.shields.io/docsrs/diotypst?style=flat-square)](https://docs.rs/diotypst)

**Typst integration primitives for Dioxus apps.**

Rendering is explicit end to end: a Typst Project (root entrypoint plus explicit files) renders inside an explicit Render Environment (Package Bundles, Font Set, render date, System Inputs) through a crate-owned Project World. Nothing reads the host filesystem or fetches packages implicitly.

## Install

```toml
[dependencies]
diotypst = { version = "0.1", features = ["bundled-fonts", "pdf"] }
```

## Quick Start

The smallest complete flow is: create a Typst Project, render it inside an explicit Render Environment, and use the Render Artifact.

```rust
# #[cfg(all(feature = "bundled-fonts", feature = "pdf"))]
# {
use diotypst::{render_pdf, RenderEnvironment, DocumentWorkspace};

let project = DocumentWorkspace::from_source("= Title\n\nHello from Typst.");
let environment = RenderEnvironment::builder()
    .build()
    .expect("render environment should be valid");

let pdf = render_pdf(&project, &environment).expect("PDF render should succeed");

assert!(pdf.bytes().starts_with(b"%PDF-"));
# }
```

## Feature Flags

- `pdf`, `page-images`, `html`: opt-in Render Capabilities, forwarded to typst-project; see
  its feature docs.
- `bundled-fonts`: include Typst's standard text, math, and monospace fonts. Omit it
  when the application provides an explicit `FontSet` at runtime.
- `dioxus`: the Dioxus-facing render API — Render Sessions, the `Typst` component, and
  `TypstProvider`.
- `serde`: serializable projects, environments, and Package Policies.
- `server`: the Axum-compatible Server Render Route and package proxy router.
- `archive` (wasm-safe): parse verbatim Typst Universe `.tar.gz` archives into Package
  Bundles.
- `fs-packages` (native): serve unpacked packages from Typst CLI-style package
  directories.
- `download` (native): download packages from a Typst Universe-style registry.
- `system-downloader` (native): the built-in native HTTPS downloader from typst-kit.
- `pack` (wasm-safe): read and write portable `.typk` Project Pack archives.
- `vendor` (native): pre-download verbatim package archives for embedding.
- `lazy-packages`: opt-in synchronous mid-render package resolution; see ADR 0008.

## Stable Flows

### Create A Typst Project

A Typst Project is one root Typst entrypoint plus the explicit Project Files available to that entrypoint. Rendering happens through an explicit Project World: the renderer sees only the Typst Project, prepared Package Bundles, the configured Font Set, and the configured Render Date. It does not read the host filesystem or fetch packages from the network implicitly.

```rust
use diotypst::DocumentWorkspace;

let project = DocumentWorkspace::builder("main.typ")
    .source_file("main.typ", "= Main\n\n#include \"chapters/intro.typ\"")
    .source_file("chapters/intro.typ", "Included from the project.")
    .build()
    .expect("project should be valid");

assert_eq!(project.root_path().get_without_slash(), "main.typ");
assert_eq!(
    project.file_bytes("chapters/intro.typ"),
    Some("Included from the project.".as_bytes())
);
```

### Configure The Render Environment

`RenderEnvironment` carries non-source rendering context. With the `bundled-fonts` feature it uses Typst's bundled fonts; without that feature its Font Set is empty. It always uses the deterministic Render Date `2026-07-01`. Configure it when the application needs a specific Font Set, Render Date, or prepared Package Bundle.

Packages are explicit too. Use exact package versions such as `@preview/example:1.2.3`; non-exact or `latest` references are rejected before rendering.

```rust
use diotypst::{PackageBundle, PackageSpec, RenderDate, RenderEnvironment};
use std::str::FromStr;

let spec = PackageSpec::from_str("@preview/example:1.2.3")
    .expect("exact package spec should parse");
let bundle = PackageBundle::builder(spec.clone())
    .file(
        "typst.toml",
        b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n".to_vec(),
    )
    .file("lib.typ", b"#let answer = [Imported explicitly.]".to_vec())
    .build()
    .expect("package bundle should be valid");
let render_date = RenderDate::from_ymd(2024, 1, 2).expect("date should be valid");

let environment = RenderEnvironment::builder()
    .package_bundle(bundle)
    .render_date(render_date)
    .build()
    .expect("render environment should be valid");

assert!(environment.package_bundle(&spec).is_some());
assert_eq!(environment.render_date(), render_date);
```

Use package dependency observation as a preflight aid for cache warming or template validation. Observed packages come from one compile pass; they do not replace a template's declared package list.

```rust
# #[cfg(feature = "html")]
# {
use diotypst::{
    observe_package_dependencies, PackageDependencyTarget, RenderEnvironment, DocumentWorkspace,
};

let project = DocumentWorkspace::from_source(
    "#import \"@preview/example:1.2.3\": answer\n#answer",
);
let environment = RenderEnvironment::builder()
    .build()
    .expect("render environment should be valid");

let observation = observe_package_dependencies(
    &project,
    &environment,
    PackageDependencyTarget::Html,
)
.expect("project should be valid enough for preflight");

assert_eq!(observation.packages()[0].to_string(), "@preview/example:1.2.3");
assert!(!observation.compile_succeeded());
# }
```

### Prepare Packages Through A Package Source

`prepare_packages` runs Package Preparation during World Preparation: it preflight-compiles the Typst Project, resolves the observed packages through an explicit Package Source, and repeats until nothing is missing (packages can import further packages). Sources include in-memory bundles, embedded `.tar.gz` archives (feature `archive`), package directories (feature `fs-packages`), and Typst Universe downloads (feature `download`); gate any source with an allowlist/denylist `PackagePolicy`.

```rust
# #[cfg(feature = "html")]
# {
use diotypst::{
    prepare_packages, GatedPackages, MemoryPackages, PackageBundle, PackageDependencyTarget,
    PackagePolicy, PreparePackagesOptions, RenderEnvironment, DocumentWorkspace,
};

let bundle = PackageBundle::builder("@preview/example:1.2.3".parse().expect("spec should parse"))
    .file(
        "typst.toml",
        b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n".to_vec(),
    )
    .file("lib.typ", b"#let answer = [Prepared explicitly.]".to_vec())
    .build()
    .expect("package bundle should be valid");
let source = GatedPackages::new(
    MemoryPackages::new([bundle]).expect("package source should be valid"),
    PackagePolicy::deny_all().allow("@preview/example".parse().expect("pattern should parse")),
);
let project = DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\": answer\n#answer");
let environment = RenderEnvironment::builder()
    .build()
    .expect("render environment should be valid");

let preparation = tokio::runtime::Builder::new_current_thread()
    .build()
    .expect("runtime should build")
    .block_on(prepare_packages(
        &project,
        &environment,
        PackageDependencyTarget::Html,
        &source,
        PreparePackagesOptions::new(),
    ))
    .expect("preparation should run");

assert!(preparation.fixed_point());
let environment = preparation.into_environment();
# }
```

Unresolved packages are recorded per spec instead of failing preparation; the subsequent render surfaces Typst's own package diagnostics. On the web, `FetchPackageSource` (feature `archive`) downloads archives with the browser `fetch` API, either directly from a registry or through the fullstack package proxy (`server_package_proxy_router`, feature `server`), which enforces its Package Policy server-side. A Render Session (feature `dioxus`) runs this loop reactively with per-package progress.

### Render An Artifact

Use the typed helpers when the format is fixed, or `render_artifact` when a Dioxus UI lets the user choose between PDF, Page Images, and semantic HTML.

```rust
# #[cfg(all(
#     feature = "bundled-fonts",
#     feature = "html",
#     feature = "page-images"
# ))]
# {
use diotypst::{
    render_artifact, PageImageOptions, RenderArtifact, RenderEnvironment, RenderFormat,
    DocumentWorkspace,
};

let environment = RenderEnvironment::builder()
    .build()
    .expect("render environment should be valid");

let html_project = DocumentWorkspace::from_source("= Title\n\nHello from Typst.");
let html_artifact = render_artifact(&html_project, &environment, RenderFormat::Html)
    .expect("HTML render should succeed");
let RenderArtifact::Html(html) = html_artifact else {
    panic!("expected HTML artifact");
};

assert!(html.as_str().starts_with("<!DOCTYPE html>"));
assert!(html.as_str().contains("<p>Hello from Typst.</p>"));

let page_project = DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page.");
let page_artifact = render_artifact(
    &page_project,
    &environment,
    RenderFormat::PageImages(PageImageOptions::default()),
)
.expect("Page Image render should succeed");
let RenderArtifact::PageImages(images) = page_artifact else {
    panic!("expected Page Images artifact");
};

assert_eq!(images.page_count(), 2);
assert!(images
    .page(0)
    .expect("first page image should exist")
    .bytes()
    .starts_with(b"\x89PNG\r\n\x1a\n"));
# }
```

### Render A Complete Typst World

Use the raw world helpers when the application already owns a complete Typst `World`, or when it wants to construct a `SandboxedWorld` and apply explicit overlays before rendering.

```rust
# #[cfg(all(feature = "bundled-fonts", feature = "html"))]
# {
use diotypst::{
    render_html_world, SandboxedWorld, RenderEnvironment, DocumentWorkspace, WorldOverlay,
};

let project = DocumentWorkspace::builder("main.typ")
    .source_file("main.typ", "#include \"content.typ\"")
    .source_file("content.typ", "Base content.")
    .build()
    .expect("project should be valid");
let environment = RenderEnvironment::builder()
    .build()
    .expect("render environment should be valid");
let base_world = SandboxedWorld::for_html(project, environment)
    .expect("Project World should be valid");
let world = WorldOverlay::new(base_world)
    .source_file("content.typ", "Overlay content.")
    .expect("overlay path should be valid");

let html = render_html_world(&world).expect("overlay world should render");

assert!(html.as_str().contains("<p>Overlay content.</p>"));
# }
```

### Keep Render State And Prepare Downloads

`RenderState` keeps the latest successful Render Artifact as a Stale Artifact when a later render fails. `DownloadFile` prepares downloadable bytes from the current or stale artifact. PDF and Page Images are downloadable; HTML artifacts are preview-only and cannot be downloaded.

```rust
# #[cfg(all(
#     feature = "bundled-fonts",
#     feature = "html",
#     feature = "page-images",
#     feature = "pdf"
# ))]
# {
use diotypst::{
    render_artifact, DownloadError, DownloadFile, PageImageOptions, RenderEnvironment,
    RenderFormat, RenderState, RenderStatus, DocumentWorkspace,
};

let environment = RenderEnvironment::builder()
    .build()
    .expect("render environment should be valid");

let mut pdf_state = RenderState::new();
pdf_state.update(render_artifact(
    &DocumentWorkspace::from_source("= Current\n\nStill downloadable."),
    &environment,
    RenderFormat::Pdf,
));
pdf_state.update(render_artifact(
    &DocumentWorkspace::from_source("#let broken ="),
    &environment,
    RenderFormat::Pdf,
));

let pdf_download = DownloadFile::from_render_artifact_state("document.pdf", &pdf_state)
    .expect("stale PDF artifact should be downloadable");

assert_eq!(pdf_state.status(), RenderStatus::Stale);
assert_eq!(pdf_download.media_type(), "application/pdf");
assert!(pdf_download.bytes().starts_with(b"%PDF-"));

let mut page_state = RenderState::new();
page_state.update(render_artifact(
    &DocumentWorkspace::from_source("First page.\n#pagebreak()\nSecond page."),
    &environment,
    RenderFormat::PageImages(PageImageOptions::default()),
));

let page_download = DownloadFile::from_render_artifact_state("pages.zip", &page_state)
    .expect("Page Images artifact should be downloadable as an archive");

assert_eq!(page_download.media_type(), "application/zip");
assert!(page_download.bytes().starts_with(b"PK\x03\x04"));

let mut html_state = RenderState::new();
html_state.update(render_artifact(
    &DocumentWorkspace::from_source("= Preview Only"),
    &environment,
    RenderFormat::Html,
));

let error = DownloadFile::from_render_artifact_state("document.html", &html_state)
    .expect_err("HTML artifacts are not downloadable");

assert_eq!(error, DownloadError::UnsupportedArtifact);
# }
```

### Pack A Project Into A Portable Archive

Enable the `pack` feature (wasm-safe) to read and write Project Packs: single-file `.typk` archives of a whole Typst Project, defined by the independent [`typst-pack`](https://github.com/sagikazarmark/typst-pack) crate. A pack carries the project files, vendored Package Bundles, external package specs, and optional embedded font files, so a project can leave one app and render offline in another. Vendored Typst Universe packages remain verbatim `.tar.gz` archives on the registry side; the pack is the project-level exchange format.

```rust
# #[cfg(all(feature = "bundled-fonts", feature = "html", feature = "pack"))]
# {
use diotypst::{render_html, DownloadFile, PackageBundle, ProjectPack, DocumentWorkspace};

let project = DocumentWorkspace::from_source("#import \"@preview/example:1.2.3\": answer\n#answer");
let bundle = PackageBundle::builder("@preview/example:1.2.3".parse().expect("spec should parse"))
    .file(
        "typst.toml",
        b"[package]\nname = \"example\"\nversion = \"1.2.3\"\nentrypoint = \"lib.typ\"\n".to_vec(),
    )
    .file("lib.typ", b"#let answer = [Vendored in the pack.]".to_vec())
    .build()
    .expect("package bundle should be valid");

let pack = ProjectPack::builder(project)
    .package_bundle(bundle)
    .build()
    .expect("pack should be valid");
let download = DownloadFile::from_project_pack("project.typk", &pack)
    .expect("pack should serialize");

// Load a pack back anywhere, including wasm, and render fully offline.
let pack = ProjectPack::from_bytes(download.bytes()).expect("pack should parse");
let environment = pack
    .render_environment()
    .expect("pack environment should be valid");
let html = render_html(pack.project(), &environment).expect("pack should render offline");

assert!(html.as_str().contains("Vendored in the pack."));
# }
```

Packs whose dependencies are not all vendored report them through `ProjectPack::external_packages`; resolve those through a Package Source with `prepare_packages` starting from `pack.render_environment()`.

## Dioxus And Server Flows

Render Capabilities are opt-in features forwarded to typst-project — `pdf`, `page-images`, and
`html` — so an HTML-preview wasm build can omit the PDF exporter and the raster renderer;
requesting an absent capability is an explicit `RenderError::UnsupportedFormat`, never a
silent fallback.

Enable the `dioxus` feature to use the Dioxus-facing render API. `use_render_session` is the main entry point, and it is declarative: because rendering is deterministic (explicit Render Environment, explicit Font Set, fixed Render Date), a Render Session is reactive memoization of a pure function. It renders synchronously on mount, re-renders whenever the Typst Project, view, or Render Environment it is given changes, runs World Preparation through the configured Package Source (degrading to missing-package diagnostics instead of blocking, then re-rendering when the resolved Package Bundles land), and retains the last good artifact as a Stale Artifact when newer source has errors.

The Render Policy is the caller's signal wiring — the session renders whatever value reaches it, so the app decides what reaches it: pass a signal committed on a button press for explicit rendering, or a debounced signal (for example [`dioxus-sdk-time`](https://crates.io/crates/dioxus-sdk-time)'s `use_debounce`) for live preview. Keep the *live* signal on the editor widget and feed the session the committed or debounced one; rendering is synchronous CPU work, so avoid raw keystrokes for non-trivial documents. `TypstProvider` supplies shared defaults (Render Environment, Package Source), each overridable per session through `RenderSessionOptions`. The high-level `Typst` component is a thin view over a session, and `use_typst_render` remains the lower escape hatch for rendering custom Complete Typst Worlds.

```rust,ignore
use dioxus::prelude::*;
use diotypst::{Typst, TypstInput, TypstProvider, TypstProviderDefaults, TypstView};

#[component]
fn App() -> Element {
    let defaults = TypstProviderDefaults::default();

    rsx! {
        TypstProvider { defaults,
            Typst {
                input: TypstInput::source("= Hello\n\nRendered by the Typst component."),
                view: TypstView::Html,
            }
        }
    }
}
```

```rust,ignore
use diotypst::{RenderSessionOptions, TypstInput, TypstView, use_render_session};

// Explicit rendering as signal wiring: the editor shows `editor`, the session
// reads `committed`, and a Render button does `committed.set(editor())`.
let session = use_render_session(
    TypstInput::source(committed.read().clone()),
    TypstView::Html,
    RenderSessionOptions::new(),
);
// session.state()               -> Render State: current/stale artifact + diagnostics
// session.preparation()         -> World Preparation phase + per-package progress
// session.restart_preparation() -> retry failed package resolutions
```

Enable the `server` feature to mount an Axum-compatible Server Render Route at `/typst/render-download`:

```rust,ignore
let router = axum::Router::new().merge(diotypst::server_render_download_router());
```

The route accepts a `ServerRenderRequest` containing a Typst Project (`DocumentWorkspace`), a Render Environment, the requested download format, and a suggested filename; with the `serde` feature the project and environment are validated while deserializing the request. It renders through an explicit Project World and returns downloadable PDF (`application/pdf`), Page Image (`image/png`), or Page Image Archive (`application/zip`) responses. HTML artifacts are rejected as unsupported downloads.

## Demo

The [demo](https://github.com/sagikazarmark/diotypst/tree/main/demo) is a Dioxus app that shows the headless API in a UI shell. The web build renders semantic HTML through an explicit Render Now action or an opt-in debounced live preview, then prepares client-side PDF and Page Image Archive downloads. The server build posts Typst Project input to a Server Render Route for PDF and Page Image Archive downloads. It deploys as a Cloudflare Worker serving the static SPA plus the package proxy.

```bash
cd demo
npm ci
npm run build
dagger call fonts export --path ./public/fonts
dx serve --fullstack \
  @client --platform web --no-default-features --features web \
  @server --platform server --no-default-features --features server
```

## Related Crates

- [`typst-project`](https://crates.io/crates/typst-project): the Dioxus-independent core that
  constructs Project Worlds from explicit Typst Projects, Package Bundles, Font Sets,
  and render dates, and renders them to artifacts. `diotypst` depends on it and
  re-exports its API so Dioxus-facing flows can keep using one import path.
- [`typst-package-source`](https://crates.io/crates/typst-package-source): the
  package-acquisition tier (Package Sources, Bundles, and Policies), re-exported
  through `typst-project`.

See the [workspace README](https://github.com/sagikazarmark/diotypst) for the full
documentation, design terminology, and live examples.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
