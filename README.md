# diotypst

[![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/sagikazarmark/diotypst/dagger.yaml?style=flat-square)](https://github.com/sagikazarmark/diotypst/actions/workflows/dagger.yaml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/sagikazarmark/diotypst/badge?style=flat-square)](https://securityscorecards.dev/viewer/?uri=github.com/sagikazarmark/diotypst)
[![crates.io](https://img.shields.io/crates/v/diotypst?style=flat-square)](https://crates.io/crates/diotypst)
[![docs.rs](https://img.shields.io/docsrs/diotypst?style=flat-square)](https://docs.rs/diotypst)

**Typst integration primitives for Dioxus apps.**

> [!WARNING]
> This project is in early development and may change without notice.

Rendering is explicit end to end: a **Typst Project** (root entrypoint plus explicit
files) renders inside an explicit **Render Environment** (Package Bundles, Font Set,
render date, System Inputs) through a crate-owned **Project World**. Nothing reads the
host filesystem or fetches packages implicitly.

## Features

- **Explicit Project Worlds** — a complete Typst `World` built from an explicit Typst
  Project and Render Environment; no implicit filesystem or network access mid-render.
- **Three Render Artifacts** — PDF, Page Images, and semantic HTML, each backend an
  opt-in feature, with structured diagnostics and stale-artifact handling.
- **Explicit Package Sources** — in-memory bundles, embedded archives, package
  directories, and Typst Universe downloads, composable in chains and gated by an
  allowlist/denylist Package Policy.
- **Wasm-safe by construction** — the in-memory path (including archive parsing and
  `.typk` Project Packs) compiles for the browser; native-only pieces are features.
- **Dioxus integration** — the declarative Render Session hook, the `Typst` component,
  Render State, and browser downloads.
- **Fullstack server routes** — the Server Render Route and the policy-gated package
  proxy, mountable on any Axum router.

## Quick Start

Add the flagship crate with the capabilities your Dioxus app needs:

```toml
[dependencies]
diotypst = { version = "0.1", features = ["bundled-fonts", "dioxus", "html"] }
```

See the [`diotypst` Quick Start](crates/diotypst/README.md#quick-start) for a complete
rendering example.

## Development

Minimum verification:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`

Or run the same checks in a container with [Dagger](https://dagger.io), exactly as CI
does:

- `dagger check`: from the repo root for the workspace, or from `demo/` for the demo app

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
