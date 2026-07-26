# typst-embed

[![crates.io](https://img.shields.io/crates/v/typst-embed?style=flat-square)](https://crates.io/crates/typst-embed)
[![docs.rs](https://img.shields.io/docsrs/typst-embed?style=flat-square)](https://docs.rs/typst-embed)

**Embed Typst in an application: explicit projects, worlds, and rendering across native and wasm targets.**

This crate owns the target-agnostic path from an explicit Typst Project and Render Environment to a complete Typst `World` and its Render Artifacts (PDF, Page Images, semantic HTML), including diagnostics and the package preparation loop. It does not read host files or fetch packages implicitly during Typst world lookup. It extends [typst-kit](https://docs.rs/typst-kit) with the explicit, in-memory, wasm-safe path that closed-world and in-browser rendering need.

## Install

```toml
[dependencies]
typst-embed = "0.1"
```

## Quick Start

```rust
use typst_embed::{Project, RenderEnvironment};

let project = Project::from_source("= Hello");
let environment = RenderEnvironment::builder()
    .input("customer", "Acme")
    .build()
    .expect("render environment should be valid");

let world = environment.world(project)
    .expect("project should be valid");
```

Render Environment clones share immutable package storage and lazily prepared font state, so one
environment can cheaply build worlds for changing editor projects. Use
`environment.world_builder(project).html().build()` when the world must enable Typst's HTML
feature. A stateful editor can retain one `ProjectWorld` and call `replace_project` between
renders; its file store edits previously requested Typst sources in place for incremental
compilation while the root and Project File paths remain stable. Project layout changes replace
the file store without rebuilding prepared environment resources.

## Feature Flags

- `pdf`, `page-images`, `html`: opt-in Render Capability backends; see below.
- `bundled-fonts`: include Typst's standard fonts; omit it when the application
  supplies an explicit Font Set at runtime.
- `serde`: serializable projects, environments, and Package Policies.
- `archive` (wasm-safe): parse verbatim Typst Universe `.tar.gz` archives into Package
  Bundles.
- `fs-packages` (native): serve unpacked packages from Typst CLI-style package
  directories.
- `download` (native): download packages from a Typst Universe-style registry.
- `system-downloader` (native): the built-in native HTTPS downloader from typst-kit.
- `pack` (wasm-safe): read and write portable `.typk` Project Pack archives.
- `vendor` (native): pre-download verbatim package archives for embedding.
- `lazy-packages`: opt-in synchronous mid-render package resolution; see ADR 0008.

## Domain Model

- `Project` is the document input: one root Typst entrypoint plus explicit Project Files addressed by normalized root-relative paths.
- `RenderEnvironment` is non-source render context: Package Bundles, fonts, deterministic render date, and Typst `sys.inputs`.
- `ProjectWorld` is the crate-owned complete Typst `World` built from an explicit Typst Project and Render Environment.
- `WorldOverlay` layers exact resource overrides over an existing complete world without mutating the base world.

## Render Capabilities

Each Render Artifact format's backend is an opt-in feature: `pdf`, `page-images`, and
`html`, so builds carry only the render backends they use. The typed functions
(`render_pdf`, …) exist only with their feature; `render_artifact` stays available on every
build and reports an absent backend as an explicit `RenderError::UnsupportedFormat`, never a
silent fallback.

## Package Sources

Packages resolve through explicit Package Sources during World Preparation, before world
construction, and land in the Render Environment as in-memory Package Bundles. Rendering never
fetches. Sources compose in ordered chains and can be gated by an allowlist/denylist
`PackagePolicy`.

The whole package layer lives in the [`typst-package-source`] crate, a typst-syntax-tier
dependency, so package tooling (registries, proxies, vendoring, serverless workers) compiles
without the Typst compiler, and is re-exported here in full:

[`typst-package-source`]: https://crates.io/crates/typst-package-source

```rust
use typst_embed::{
    GatedPackages, MemoryPackages, PackageBundle, PackagePolicy, SyncPackageSource,
};

let bundle = PackageBundle::builder("@preview/example:1.0.0".parse().expect("spec should parse"))
    .file("typst.toml", "[package]\nname = \"example\"\nversion = \"1.0.0\"\nentrypoint = \"lib.typ\"\n")
    .file("lib.typ", "#let answer = 42")
    .build()
    .expect("package bundle should be valid");
let source = GatedPackages::new(
    MemoryPackages::new([bundle]).expect("package source should be valid"),
    PackagePolicy::deny_all().allow("@preview/example".parse().expect("pattern should parse")),
);

let resolved = source.resolve_sync(&"@preview/example:1.0.0".parse().expect("spec should parse"))
    .expect("allowed package should resolve");
```

Available sources:

- `MemoryPackages`: explicit in-memory Package Bundles.
- `PackageBundle::from_tar_gz` (feature `archive`, wasm-safe): parse verbatim Typst Universe
  `.tar.gz` archives, typically embedded with `include_bytes!`.
- `FsPackages` (feature `fs-packages`, re-exported from typst-kit): unpacked packages from a
  Typst CLI-style package directory, including the system data and cache directories.
- `SystemPackages` (feature `fs-packages`, re-exported from typst-kit): the full Typst CLI
  resolution chain as one source: data directory, then cache directory, then a Typst Universe
  download stored into the cache.
- `RegistryPackages` (feature `download`): download from Typst Universe or a mirror through a
  typst-kit `Downloader`, optionally retaining downloads in an `FsPackages` cache. Missing
  versions of known packages report `VersionNotFound` with the latest version from the package
  index, like typst-cli. The `system-downloader` feature provides a built-in native HTTPS
  downloader.
- `vendor_package_archives` (feature `vendor`): pre-download verbatim archives for embedding.

The async `PackageSource` trait is the World Preparation seam (browser fetch implementations
live in `diotypst`); `SyncPackageSource` additionally supports the opt-in
`lazy-packages` mid-render resolution described in ADR 0008.

## Project Packs

The `pack` feature (wasm-safe) reads and writes Project Packs: single-file `.typk` archives of a
whole Typst Project defined by the independent
[`typst-pack`](https://github.com/sagikazarmark/typst-pack) crate. A pack carries the project
files, exact package-tree and font-container requirements, and any vendored resources, so it
converts straight into this crate's domain types:

```rust
# #[cfg(feature = "pack")]
# {
use typst_embed::{Project, ProjectPack};

let pack = ProjectPack::builder(Project::from_source("= Portable"))
    .build()
    .expect("pack should be valid");
let bytes = pack.to_bytes().expect("pack should serialize");

let pack = ProjectPack::from_bytes(&bytes).expect("pack should parse");
let environment = pack
    .render_environment()
    .expect("pack environment should be valid");

assert_eq!(pack.project().root_path().get_without_slash(), "main.typ");
# let _ = environment;
# }
```

`ProjectPack::render_environment` is the self-contained shortcut: it installs vendored packages
and embedded fonts, and reports any external requirement that remains. When an app already has a
Render Environment containing deliberately unvendored resources, use
`pack.render_environment_from(&base)`. The base supplies exact package and font fulfillments plus
the Render Date and System Inputs; undeclared ambient resources do not enter the result.

`ProjectPack::environment_builder` is the extensible path for supplying Package Bundles, font
containers, or Render Context separately. `ProjectPackBuilder::external_package_bundle` and
`ProjectPackBuilder::external_font_file` inspect complete resources to establish their identities
without storing their bytes in the archive.

## Overlays

Overlays shadow exact resources before delegating unresolved requests to the base world. Project files match by project path, package bundles match by exact package spec, and overlay render dates only affect calls through that overlay.

```rust
use typst_embed::{ProjectWorld, WorldOverlay};

# let project = typst_embed::Project::from_source("Base");
# let environment = typst_embed::RenderEnvironment::builder().build().unwrap();
# let base = ProjectWorld::new(project, environment).unwrap();
let overlay = WorldOverlay::new(base)
    .source_file("preview.typ", "Overlay main")?
    .main("preview.typ")?;
# Ok::<_, typst_embed::ProjectValidationError>(())
```

## Related Crates

- [`typst-package-source`](https://crates.io/crates/typst-package-source): the
  package-acquisition tier, re-exported here in full.
- [`diotypst`](https://crates.io/crates/diotypst): the Dioxus-facing crate built on this
  one; it re-exports the whole `typst-embed` API.

See the [workspace README](https://github.com/sagikazarmark/diotypst) for the full
documentation, design terminology, and live examples.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
