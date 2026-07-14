# Dioxus Typst

Typst rendering concepts used by this crate and its demo.

## Language

**Typst Project** (`DocumentWorkspace`):
A renderable Typst document input: one root Typst entrypoint plus explicit files, assets, and data addressed by Project Paths.
_Avoid_: Document Bundle, loose source string, resolved packages

**Root Entrypoint**:
The Project Path that Typst treats as `World::main` for a Typst Project.
_Avoid_: Main file embedded in a loader, ambient current document

**Project File** (`WorkspaceFile`):
A named byte resource inside a Typst Project, addressed by a Project Path and available for includes, images, fonts, or data loading.
_Avoid_: loose file, external asset, host file

**Project Path** (typst's `VirtualPath`):
A normalized Typst path inside a Typst Project; rooted and root-relative spellings normalize to the same path, and escaping paths are rejected.
_Avoid_: parent-directory escape, host path

**File Loader**:
A synchronous lookup seam that returns Project File bytes by Project Path. Typst source files and binary files use the same byte resource; the Root Entrypoint is configured separately from the loader.
_Avoid_: Source Loader, Resource Loader, main-file loader, host filesystem adapter

**World Preparation**:
All work that must finish before Typst rendering can read resources synchronously, such as DB or object-store reads, browser fetch/cache, package download, and font loading.
_Avoid_: Async World lookup, lazy network world, implicit package world

**Render Environment**:
The explicit non-source context used while rendering, including Package Bundles, the Font Set, the render date, and System Inputs.
_Avoid_: Ambient host environment, hidden render state

**System Inputs**:
Explicit Typst values installed into the Typst library and visible to document code through `sys.inputs`.
_Avoid_: Environment variables, CLI args, ambient application state

**Complete Typst World**:
A Typst World that can be rendered directly because it supplies its root entrypoint, Typst library features, System Inputs, and the source, file, package, font, and date resources Typst may request.
_Avoid_: Source string plus world, incomplete world

**Project World** (`SandboxedWorld`):
The crate-owned Complete Typst World built from a Typst Project, its Render Environment, explicit Typst library features, and prepared resources.
_Avoid_: TypstWorld, generic world, WorkspaceWorld, hidden workspace renderer

**World Overlay**:
A layered Typst World that supplies explicit document resources, exact resource replacements, or selected render-context overrides before delegating unresolved requests to a base Complete Typst World. Overlay precedence is explicit; it is not mutation of the base world.
_Avoid_: Implicit merge, mutable base world, hidden override

**Render Artifact**:
Output produced from a Typst Project, such as a PDF, Page Image, or HTML Artifact.
_Avoid_: Preview blob, output file

**Stale Artifact**:
The latest successful Render Artifact shown while the current Typst Project has not rendered successfully.
_Avoid_: Current preview, hidden failure

**Render Policy**:
The app-level rule that decides when a Typst Project should be rendered, realized as signal wiring: the value that reaches a Render Session's input — committed on an explicit user action, or debounced from a live editing stream — is what renders. It is not a library scheduling mechanism.
_Avoid_: Hidden rerendering, feeding raw keystrokes to a Render Session, debouncing the signal the editor widget displays

**Render Session** (`RenderSession`):
The declarative flow that keeps one Typst Project rendered: World Preparation and Render State behind one handle. Because rendering is deterministic, a session is reactive memoization of a pure function — it renders synchronously on mount and re-renders whenever its Typst Project, view, or Render Environment changes; preparation enrichment re-renders with the resolved Package Bundles. There is no imperative render trigger.
_Avoid_: Ambient render loop, hidden rerendering, imperative render triggers, hand-chained preparation and render hooks

**Render Capability**:
An artifact format that is available for a given target and feature set. Mechanically, each format's backend is a build feature (`pdf`, `page-images`, `html`); requesting an absent capability is an explicit error.
_Avoid_: Silent fallback, hidden unsupported format

**Page Image**:
A raster image of one rendered Typst page. Multi-page PNG output is a collection of page images, not one stitched image.
_Avoid_: PNG document, stitched image

**Page Image Options**:
The requested fidelity settings for Page Image output, such as scale or DPI.
_Avoid_: Global PNG resolution, CSS-inferred download quality

**Page Image Archive**:
A downloadable archive containing one Page Image per rendered Typst page.
_Avoid_: Multi-page PNG, repeated browser downloads

**HTML Artifact**:
A self-contained semantic HTML rendering of a Typst Project. It is not expected to be pixel-equivalent to PDF output or Page Images, and user-authored HTML artifacts should be treated as untrusted when embedded.
_Avoid_: HTML page preview, pixel-perfect HTML

**Package Source**:
An explicit place a Typst Project may use during World Preparation to resolve Typst packages: the `PackageSource` trait plus implementations such as in-memory bundles, embedded archives, package directories, and registry downloads, composable in ordered chains.
_Avoid_: Implicit network fetch, automatic package download

**Package Policy**:
An explicit allowlist/denylist deciding which packages a Package Source may resolve, matching at namespace, name, or exact-version granularity with deny winning over allow.
_Avoid_: Hidden download gate, implicit trust in a registry

**Package Preparation**:
The World Preparation fixed-point loop that preflight-compiles a Typst Project, resolves Observed Package Dependencies through a Package Source, and repeats until nothing is missing, no progress is made, or an iteration cap is reached.
_Avoid_: Unlimited recompiles, mid-compile fetch by default

**Vendored Package Archive**:
A verbatim Typst Universe `.tar.gz` archive downloaded ahead of time for embedding into a binary, serving from disk, or proxying; never re-packed.
_Avoid_: Re-packed archive, unpacked loose files as the exchange format

**Project Pack**:
A portable single-file `.typk` archive of a whole Typst Project: project files, vendored Package Bundles, external package specs, and optional embedded font files. The format belongs to the independent `typst-pack` crate; packages inside a pack are project-level cargo, not a replacement for Vendored Package Archives on the registry side.
_Avoid_: Project bundle, re-packed Universe archive, tar.gz project export

**Lazy Package Resolution**:
The explicit opt-in exception (feature flag plus builder call) that lets a native Project World resolve packages synchronously mid-render through a synchronous Package Source.
_Avoid_: Default behavior, ambient network during world lookup

**Package Bundle**:
A set of package files identified by an exact Typst package spec.
_Avoid_: Loose package files, latest package alias

**Observed Package Dependency**:
An exact Package Spec that Typst requested during a preflight compile or render. It is evidence from one run, not the canonical package contract for a Typst Project or template.
_Avoid_: Declared package, required package, complete dependency graph

**Project Import**:
A user action that creates a Typst Project from selected files or a selected directory and identifies one Root Entrypoint.
_Avoid_: Single-file upload, raw source paste

**Project Validation**:
Checks that a Typst Project can be rendered as a coherent unit, such as requiring the Root Entrypoint to exist and Project Paths to be unique.
_Avoid_: Silent overwrite, implicit empty root

**Font Set**:
The explicit collection of fonts available while rendering a Typst Project.
_Avoid_: Ambient system fonts, hidden browser fonts

**Headless Component**:
A Dioxus component that provides Typst rendering or download behavior without prescribing visual styling or fixed markup.
_Avoid_: Styled widget, demo-only component

**Typst Provider**:
A Dioxus context provider for shared Typst rendering defaults and services, such as a Render Environment, backend choice, policy defaults, or named presets. It does not own the current document unless explicitly modeled as a document-specific provider.
_Avoid_: Ambient current world, hidden document owner

**Typst Component**:
A high-level Dioxus component that renders one document input as a selected view format, such as semantic HTML, a PDF frame, or Page Images, using provider defaults when present.
_Avoid_: Headless render state, download action, implicit format switch

**Diagnostic**:
A Typst warning or error surfaced during rendering, ideally tied to a Typst source identity and source range. Project-built worlds may additionally map that identity to a Project Path.
_Avoid_: Plain error string, silent render failure, workspace-only location

**Download Action**:
A user-triggered request to obtain a PDF, Page Image, or Page Image Archive for the current Typst Project, using a current successful render or rendering on demand.
_Avoid_: Styled download button, blind file link, HTML download

**Download Backend**:
The place a Download Action obtains its bytes from, such as client-side rendering or a Server Render Route.
_Avoid_: Separate download API, hidden backend switch

**Server Render Route**:
A Dioxus fullstack route that renders a Typst Project on the server and returns a PDF, Page Image, or Page Image Archive as a downloadable response.
_Avoid_: Standalone CLI demo, unrelated backend framework
