//! Cloudflare Worker backend for the demo's Cloudflare-SPA deployment.
//!
//! Cloudflare serves the static Dioxus bundle directly and only invokes this
//! Worker for `/typst/*` (see `wrangler.toml`'s `run_worker_first`). The one
//! route is a package proxy adapter over the shared sans-IO core
//! (`typst_package_source::ProxyArchiveRequest`): spec validation, Package
//! Policy enforcement, upstream URL construction, and the response headers all
//! come from the same code the native axum proxy uses. `typst-package-source`'s
//! base build is typst-syntax-tier, so the Worker still pulls in none of the
//! Typst compiler or rendering stack.

use typst_package_source::{
    PACKAGE_ARCHIVE_CACHE_CONTROL, PACKAGE_ARCHIVE_CONTENT_TYPE, PackageProxyError,
    ProxyArchiveRequest, UNIVERSE_REGISTRY_URL,
};
use worker::{Context, Env, Fetch, Headers, Method, Request, Response, Url, event};

use crate::packages::demo_package_policy;

/// Same-origin base path the SPA's `FetchPackageSource::proxy()` fetches from,
/// mirroring `diotypst::SERVER_PACKAGE_PROXY_BASE`.
const PACKAGE_PROXY_BASE: &str = "/typst/packages/";

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> worker::Result<Response> {
    let path = req.path();
    if let Some(rest) = path.strip_prefix(PACKAGE_PROXY_BASE) {
        if req.method() != Method::Get {
            return Response::error("method not allowed", 405);
        }
        return proxy_package_archive(rest).await;
    }

    // Everything that isn't a package proxy call is a static asset (SPA fallback).
    env.assets("ASSETS")?.fetch_request(req).await
}

/// `/typst/packages/{namespace}/{name}-{version}.tar.gz`: the Worker adapter
/// of the package proxy core. Serves verbatim archives for permitted packages
/// only.
async fn proxy_package_archive(rest: &str) -> worker::Result<Response> {
    let error =
        |failure: PackageProxyError| Response::error(format!("{failure:?}"), failure.http_status());

    let Some((namespace, archive)) = rest.split_once('/') else {
        return Response::error("invalid package archive path", 400);
    };
    let request = match ProxyArchiveRequest::parse(namespace, archive) {
        Ok(request) => request,
        Err(failure) => return error(failure),
    };
    if let Err(failure) = request.permit(&demo_package_policy()) {
        return error(failure);
    }

    let url = request.upstream_url(UNIVERSE_REGISTRY_URL);
    let mut upstream = Fetch::Url(Url::parse(&url)?).send().await?;

    match upstream.status_code() {
        200 => {
            let headers = Headers::new();
            headers.set("content-type", PACKAGE_ARCHIVE_CONTENT_TYPE)?;
            headers.set("cache-control", PACKAGE_ARCHIVE_CACHE_CONTROL)?;

            Ok(Response::from_bytes(upstream.bytes().await?)?.with_headers(headers))
        }
        404 => error(PackageProxyError::UpstreamNotFound(request.spec().clone())),
        status => error(PackageProxyError::Upstream(format!(
            "upstream fetch failed with status {status}"
        ))),
    }
}
