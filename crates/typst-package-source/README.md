# typst-package-source

[![crates.io](https://img.shields.io/crates/v/typst-package-source?style=flat-square)](https://crates.io/crates/typst-package-source)
[![docs.rs](https://img.shields.io/docsrs/typst-package-source?style=flat-square)](https://docs.rs/typst-package-source)

**Explicit, policy-gated Typst package resolution: the Package Source seam, in-memory
bundles, registries, and archives.**

Packages resolve ahead of rendering — never implicitly mid-compile — through the async
`PackageSource` trait (with `Send` futures on native targets and non-`Send` futures on
wasm) or its synchronous sibling `SyncPackageSource`. Sources compose in ordered chains,
and any source can be gated by a serializable `PackagePolicy` allowlist/denylist.

The base build depends only on `typst-syntax`, so package tooling — registries, proxies,
vendoring, CI, serverless workers — compiles without the Typst compiler or any render
backend, including on wasm.

## Install

```toml
[dependencies]
typst-package-source = "0.1"
```

## Quick Start

An explicit in-memory Package Bundle, gated by an allowlist, resolved through the
Package Source seam:

```rust
use typst_package_source::{
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

## Feature Flags

- `serde`: serializable Package Policies and Bundles.
- `archive` (wasm-safe): parse verbatim Typst Universe `.tar.gz` archives into Package
  Bundles.
- `fs-packages` (native): serve unpacked packages from Typst CLI-style package
  directories.
- `download` (native): download packages from a Typst Universe-style registry.
- `system-downloader` (native): the built-in native HTTPS downloader from typst-kit.
- `vendor` (native): pre-download verbatim package archives for embedding.

## Sources

- `MemoryPackages`: explicit in-memory `PackageBundle`s.
- `PackageBundle::from_tar_gz` (feature `archive`, wasm-safe): parse verbatim Typst
  Universe `.tar.gz` archives, typically embedded with `include_bytes!`.
- `FsPackages` (feature `fs-packages`, implemented directly on the typst-kit type):
  unpacked packages from a Typst CLI-style package directory.
- `SystemPackages` (feature `fs-packages`, typst-kit): the full Typst CLI resolution
  chain — data directory, cache directory, Universe download — as one source.
- `RegistryPackages` (feature `download`): verbatim archive downloads from Typst Universe
  or a mirror through a typst-kit `Downloader`, with an optional `FsPackages` cache and
  typst-cli-style `VersionNotFound` diagnostics from the package index.
- `vendor_package_archives` (feature `vendor`): pre-download verbatim archives for
  embedding or serving.

## Proxy Core

`ProxyArchiveRequest` is the transport-free core of a package proxy: archive-name parsing
through Typst's own spec validation, Package Policy enforcement, upstream URL
construction, and the response header constants. Adapters do the IO — an axum router
(see `diotypst`) or a fetch-based serverless handler build on the same core.

## Related Crates

This crate is the package-acquisition tier of the
[diotypst](https://github.com/sagikazarmark/diotypst) workspace:
`typst-package-source` (typst-syntax tier) → `typst-project` (worlds and rendering) →
`diotypst` (Dioxus integration). `typst-project` re-exports this crate's entire surface, so
its consumers never need a direct dependency.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
