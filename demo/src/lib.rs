//! Library half of the Dioxus Typst demo.
//!
//! The demo UI (see `main.rs`) is compiled for the `web`/`server` targets and
//! uses the [`support`] helpers re-exported here. The `cdylib` half is only the
//! Cloudflare Worker backend: it renders no pages, so it pulls in none of the
//! Typst or Dioxus rendering stack, just the package proxy route. The
//! [`packages`] module holds the package allowlist shared by every target, so
//! the native server and the Worker can never drift.

pub mod packages;

#[cfg(any(feature = "web", feature = "server"))]
mod support;
#[cfg(any(feature = "web", feature = "server"))]
pub use support::*;

#[cfg(feature = "worker")]
mod worker;
