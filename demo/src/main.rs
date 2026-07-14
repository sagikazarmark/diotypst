//! diotypst demo: binary entry point (native fullstack server + wasm SPA
//! client).
//!
//! Every page mounts a real feature *and* renders that feature's own source
//! (via the compile-time `code!` macro), so the snippet you read is exactly
//! the code that runs. The UI lives in [`app`] (router + provider),
//! [`components`] (reusable chrome and the demo shell), [`pages`] (one route
//! each), and [`examples`] (the small components the pages both mount and
//! quote).
//!
//! The native server serves the static SPA bundle plus the Typst API surface:
//! the Server Render Route, the demo download route, and the package proxy.
//! The Cloudflare Worker backend is a separate `cdylib` (see `lib.rs` /
//! `worker.rs`) serving the same SPA with the package proxy only.

mod packages;
#[cfg(any(feature = "web", feature = "server"))]
mod support;
#[cfg(any(feature = "web", feature = "server"))]
pub use support::*;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod app;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod components;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod examples;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod pages;
// dioxus-code's tree-sitter C object references libc's stderr on wasm, while
// arborium supplies the rest of the libc shims without exporting that symbol.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
#[unsafe(no_mangle)]
pub static mut stderr: *mut core::ffi::c_void = core::ptr::null_mut();

#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn main() {
    dioxus::launch(app::App);
}

#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
#[tokio::main]
async fn main() {
    use crate::demo_package_policy;
    use axum::response::IntoResponse;
    use diotypst::{
        DownloaderArchiveFetcher, PackageProxyConfig, SystemDownloader,
        server_package_proxy_router, server_render_download_router,
    };
    use dioxus_server::DioxusRouterExt;

    let address = dioxus::cli_config::fullstack_address_or_localhost();

    // The static SPA bundle, with an index.html fallback for client routes.
    let spa = axum::Router::new()
        .serve_static_assets()
        .fallback(axum::routing::get(spa_index))
        .with_state(dioxus_server::FullstackState::headless());

    let router = axum::Router::new()
        .route(
            "/typst/demo-download",
            axum::routing::post(demo_server_download),
        )
        .merge(server_render_download_router())
        .merge(server_package_proxy_router(
            PackageProxyConfig::new(demo_package_policy())
                .with_cache_dir(std::env::temp_dir().join("diotypst-demo-packages")),
            DownloaderArchiveFetcher(SystemDownloader::new(concat!(
                "diotypst-demo/",
                env!("CARGO_PKG_VERSION")
            ))),
        ))
        .merge(spa);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("server should bind to the Dioxus fullstack address");

    axum::serve(listener, router.into_make_service())
        .await
        .expect("Dioxus fullstack server should run");

    /// SPA fallback: serve the bundled `index.html` for anything that is not a
    /// static asset or an API route. Mirrors dioxus-server's (private) public
    /// path resolution: the CLI bundles static assets into `exe dir/public`.
    async fn spa_index() -> axum::response::Response {
        let public = std::env::var("DIOXUS_PUBLIC_PATH")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| Some(std::env::current_exe().ok()?.parent()?.join("public")));

        match public.and_then(|dir| std::fs::read(dir.join("index.html")).ok()) {
            Some(bytes) => (
                [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
                bytes,
            )
                .into_response(),
            None => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }

    #[derive(serde::Deserialize)]
    struct DemoServerDownloadForm {
        source: String,
        format: String,
        filename: String,
    }

    /// `/typst/demo-download`: the page-facing wrapper around the Server
    /// Render Route, accepting a plain HTML form instead of JSON.
    async fn demo_server_download(
        axum::extract::Form(form): axum::extract::Form<DemoServerDownloadForm>,
    ) -> impl IntoResponse {
        use diotypst::{
            DocumentWorkspace, DownloadFormat, PageImageOptions, ServerRenderRequest,
            server_render_download_response,
        };

        let format = match form.format.as_str() {
            "pdf" => DownloadFormat::Pdf,
            "page-image-archive" => DownloadFormat::PageImageArchive {
                options: PageImageOptions::default(),
            },
            _ => return axum::http::StatusCode::BAD_REQUEST.into_response(),
        };
        let request = ServerRenderRequest::new(
            DocumentWorkspace::from_source(form.source),
            Default::default(),
            format,
            form.filename,
        );

        match server_render_download_response(&request) {
            Ok(response) => response,
            Err(error) => error.into_response(),
        }
    }
}

// The Worker build (`--no-default-features --features worker`) compiles the
// `cdylib` in `lib.rs`; the binary is an empty stub so `cargo` still has a
// `main` to check.
#[cfg(not(any(
    all(feature = "web", target_arch = "wasm32"),
    all(feature = "server", not(target_arch = "wasm32"))
)))]
fn main() {}
