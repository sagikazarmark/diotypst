# demo

A docs-by-example gallery for
[`diotypst`](https://github.com/sagikazarmark/diotypst). Every page mounts a real
feature next to the **exact source that runs it** (rendered with the compile-time `code!`
macro from [`dioxus-code`](https://crates.io/crates/dioxus-code)), so the snippet you read
is guaranteed to be the code you see running. The whole app deploys as a Cloudflare Worker
serving the static SPA plus the package proxy.

## Build targets

The crate builds for three targets via Cargo features:

| Feature | What it is |
| --- | --- |
| `web` | Wasm SPA client; resolves packages through the same-origin package proxy. |
| `server` | Native fullstack server: serves the SPA, the Server Render Route, and the package proxy. |
| `worker` | Cloudflare Worker `cdylib`; static assets plus the package proxy route. |

Cloudflare Workers can't run the native package proxy (its downloader does not compile for
Workers), so the Worker reimplements the one route on the Worker `fetch` API.
`src/packages.rs` keeps the allowlist and path validation in one place. The server-rendered
download page needs the `server` flavor; everything else works in both deployments.

## What it covers

**Basics**: a minimal explicit render, the Typst editor with an opt-in debounced
live-preview Render Policy (including stale artifacts on errors), and System Inputs through
`sys.inputs`.

**Typst Projects**: a multi-file project built from explicit Project Files, browser
file/directory import with Root Entrypoint selection and font partitioning into the render
Font Set, and a portable `.typk` Project Pack round trip (build & download a pack with a
vendored Package Bundle, load one back, render offline).

**Packages**: Typst Universe downloads through the policy-gated same-origin package proxy
(with per-package World Preparation progress), a Package Bundle embedded into the binary as
a verbatim `.tar.gz` archive, and allowlist denial reported before any request.

**Downloads**: client-side PDF and Page Image Archive downloads prepared from current or
stale render state; HTML artifacts refuse to download.

**Server**: plain HTML forms posting to the fullstack Server Render Route for server-side
PDF and Page Image Archive downloads.

## Prerequisites

The app runs with the [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started/)
and uses npm for the Tailwind toolchain:

```sh
cargo install dioxus-cli               # if needed
npm install                            # once, for the Tailwind toolchain
```

## Run locally

```sh
dagger call fonts export --path ./public/fonts  # once, and after Typst upgrades
npm run build                          # compile assets/style.css (or: npm run watch)
dx serve --features server             # native fullstack server
dx serve --features web                # SPA only (package proxy and server routes unavailable)
```

`build/style.css` and `public/fonts/` are generated and git-ignored. Dagger exports the
standard fonts from the pinned `typst-assets` tag; npm compiles Tailwind + daisyUI. Run both
asset commands before the first `dx serve`, and rerun `npm run build` after editing RSX
classes (`npm run watch` rebuilds styles on change).

## Run with Dagger

[Dagger](https://dagger.io) builds and runs everything in containers: no local Node, `dx`,
or Wrangler needed:

```sh
dagger check                # release builds of BOTH the native fullstack app and the Worker
dagger call service up      # native fullstack, tunnelled to a local port
dagger call worker dev up   # Cloudflare Worker via `wrangler dev`
```

To deploy the Worker, pass the Cloudflare credentials explicitly:

```sh
dagger call worker deploy \
  --account-id "$CLOUDFLARE_ACCOUNT_ID" \
  --api-token env://CLOUDFLARE_API_TOKEN
```

CI deploys automatically ([`demo.yaml`](../.github/workflows/demo.yaml)): pushes to
`main` roll out to production, and pull requests upload a preview version (its URLs posted
as a PR comment). Both jobs need `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN`
repository secrets; preview only runs for same-repo PRs, since fork PRs can't read the
secrets.

## Verify

```sh
# Shared allowlist rules.
cargo test
# Support helpers (native).
cargo test --features server
# Wasm SPA client (needs assets/style.css, see above).
cargo check --no-default-features --features web --target wasm32-unknown-unknown
# Native fullstack server.
cargo check --no-default-features --features server
# Cloudflare Worker.
cargo check --no-default-features --features worker --target wasm32-unknown-unknown
```
