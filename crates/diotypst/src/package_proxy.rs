#[cfg(feature = "server")]
use crate::{PackagePolicy, PackageSpec};
pub use typst_package_source::{
    PACKAGE_ARCHIVE_CACHE_CONTROL, PACKAGE_ARCHIVE_CONTENT_TYPE, PackageProxyError,
    ProxyArchiveRequest,
};

/// Axum route path for the server package proxy endpoint.
pub const SERVER_PACKAGE_PROXY_PATH: &str = "/typst/packages/{namespace}/{archive}";

/// Fetches verbatim `.tar.gz` package archives for the server package proxy.
///
/// With the `download` feature, `DownloaderArchiveFetcher` adapts any typst-kit
/// downloader; tests and custom transports implement this
/// directly.
#[cfg(feature = "server")]
pub trait PackageArchiveFetcher: Send + Sync + 'static {
    /// Fetch the archive at `url` for the given exact Package Spec.
    fn fetch(&self, spec: &PackageSpec, url: &str) -> Result<Vec<u8>, PackageArchiveFetchError>;
}

/// A package archive fetch failure.
#[cfg(feature = "server")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageArchiveFetchError {
    /// The upstream registry does not serve this archive.
    NotFound,

    /// The upstream fetch failed, such as a network error.
    Failed(String),
}

/// A [`PackageArchiveFetcher`] backed by a typst-kit [`Downloader`](libtypst::Downloader).
#[cfg(all(feature = "server", feature = "download"))]
pub struct DownloaderArchiveFetcher<D>(pub D);

#[cfg(all(feature = "server", feature = "download"))]
impl<D: libtypst::Downloader> PackageArchiveFetcher for DownloaderArchiveFetcher<D> {
    fn fetch(&self, spec: &PackageSpec, url: &str) -> Result<Vec<u8>, PackageArchiveFetchError> {
        self.0
            .download(spec, url)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => PackageArchiveFetchError::NotFound,
                _ => PackageArchiveFetchError::Failed(error.to_string()),
            })
    }
}

/// Configuration for the server package proxy.
///
/// The proxy forwards `GET {SERVER_PACKAGE_PROXY_BASE}/{namespace}/{name}-{version}.tar.gz`
/// to an upstream Typst Universe-style registry, enforcing an explicit Package Policy
/// server-side before any upstream request.
#[cfg(feature = "server")]
#[derive(Clone, Debug, PartialEq)]
pub struct PackageProxyConfig {
    policy: PackagePolicy,
    upstream_base_url: String,
    cache_dir: Option<std::path::PathBuf>,
}

#[cfg(feature = "server")]
impl PackageProxyConfig {
    /// Create a proxy configuration with an explicit Package Policy.
    ///
    /// The policy is authoritative: denied specs are rejected with `403` before any network
    /// or disk access. The upstream defaults to the official Typst Universe registry.
    pub fn new(policy: PackagePolicy) -> Self {
        Self {
            policy,
            upstream_base_url: libtypst::UNIVERSE_REGISTRY_URL.to_owned(),
            cache_dir: None,
        }
    }

    /// Forward archive requests to a registry mirror instead of Typst Universe.
    pub fn with_upstream_base_url(mut self, url: impl Into<String>) -> Self {
        self.upstream_base_url = url.into();
        self
    }

    /// Retain fetched archives on disk at `<dir>/<namespace>/<name>-<version>.tar.gz`.
    ///
    /// Exact package versions are immutable, so cached archives are served without upstream
    /// requests; cache write failures are ignored in favor of serving the response.
    pub fn with_cache_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.cache_dir = Some(dir.into());
        self
    }

    /// Return the Package Policy enforced by this proxy.
    pub fn policy(&self) -> &PackagePolicy {
        &self.policy
    }
}

#[cfg(feature = "server")]
struct PackageProxyState {
    config: PackageProxyConfig,
    fetcher: Box<dyn PackageArchiveFetcher>,
}

/// Build an Axum router exposing the server package proxy endpoint.
///
/// The fetcher performs synchronous upstream requests inside the handler, matching the
/// synchronous rendering work of the Server Render Route.
#[cfg(feature = "server")]
pub fn server_package_proxy_router(
    config: PackageProxyConfig,
    fetcher: impl PackageArchiveFetcher,
) -> axum::Router {
    let state = std::sync::Arc::new(PackageProxyState {
        config,
        fetcher: Box::new(fetcher),
    });

    axum::Router::new()
        .route(
            SERVER_PACKAGE_PROXY_PATH,
            axum::routing::get(server_package_proxy_handler),
        )
        .with_state(state)
}

/// Map a proxy failure to its HTTP response, using the core's status mapping.
#[cfg(feature = "server")]
fn proxy_error_response(error: PackageProxyError) -> (axum::http::StatusCode, String) {
    let status = axum::http::StatusCode::from_u16(error.http_status())
        .expect("proxy status codes are valid");

    (status, format!("{error:?}"))
}

#[cfg(feature = "server")]
async fn server_package_proxy_handler(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<PackageProxyState>>,
    axum::extract::Path((namespace, archive)): axum::extract::Path<(String, String)>,
) -> Result<axum::response::Response, (axum::http::StatusCode, String)> {
    let request = ProxyArchiveRequest::parse(&namespace, &archive).map_err(proxy_error_response)?;
    request
        .permit(&state.config.policy)
        .map_err(proxy_error_response)?;

    let cache_path = state.config.cache_dir.as_ref().map(|dir| {
        dir.join(request.spec().namespace.as_str())
            .join(request.archive_filename())
    });

    if let Some(cache_path) = &cache_path
        && let Ok(bytes) = std::fs::read(cache_path)
    {
        return Ok(archive_response(bytes));
    }

    let url = request.upstream_url(&state.config.upstream_base_url);
    let bytes = state.fetcher.fetch(request.spec(), &url).map_err(|error| {
        proxy_error_response(match error {
            PackageArchiveFetchError::NotFound => {
                PackageProxyError::UpstreamNotFound(request.spec().clone())
            }
            PackageArchiveFetchError::Failed(message) => PackageProxyError::Upstream(message),
        })
    })?;

    if let Some(cache_path) = &cache_path {
        // Serving the archive matters more than retaining it; cache writes are best-effort.
        if let Some(parent) = cache_path.parent() {
            let _ =
                std::fs::create_dir_all(parent).and_then(|_| std::fs::write(cache_path, &bytes));
        }
    }

    Ok(archive_response(bytes))
}

#[cfg(feature = "server")]
fn archive_response(bytes: Vec<u8>) -> axum::response::Response {
    let mut response = axum::response::Response::new(axum::body::Body::from(bytes));

    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static(PACKAGE_ARCHIVE_CONTENT_TYPE),
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static(PACKAGE_ARCHIVE_CACHE_CONTROL),
    );

    response
}

#[cfg(all(test, feature = "server"))]
mod router_tests {
    use super::*;
    use libtypst::PackagePattern;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    /// A fetcher serving one fixture archive, recording calls.
    struct StubFetcher {
        archive: Option<Vec<u8>>,
        calls: Arc<Mutex<usize>>,
    }

    impl StubFetcher {
        fn new(archive: Option<Vec<u8>>) -> (Self, Arc<Mutex<usize>>) {
            let calls = Arc::new(Mutex::new(0));

            (
                Self {
                    archive,
                    calls: Arc::clone(&calls),
                },
                calls,
            )
        }
    }

    impl PackageArchiveFetcher for StubFetcher {
        fn fetch(
            &self,
            _spec: &PackageSpec,
            _url: &str,
        ) -> Result<Vec<u8>, PackageArchiveFetchError> {
            *self.calls.lock().expect("test lock") += 1;

            self.archive
                .clone()
                .ok_or(PackageArchiveFetchError::NotFound)
        }
    }

    fn cetz_policy() -> PackagePolicy {
        PackagePolicy::deny_all().allow(
            "@preview/cetz"
                .parse::<PackagePattern>()
                .expect("test pattern should parse"),
        )
    }

    async fn proxy_get(
        router: axum::Router,
        path: &str,
    ) -> (axum::http::StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let response = router
            .oneshot(
                axum::http::Request::builder()
                    .uri(path)
                    .body(axum::body::Body::empty())
                    .expect("test request should build"),
            )
            .await
            .expect("proxy request should produce a response");

        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("test body should read")
            .to_vec();

        (status, headers, body)
    }

    #[tokio::test]
    async fn proxy_serves_allowed_archives_with_immutable_caching() {
        let (fetcher, _) = StubFetcher::new(Some(b"archive-bytes".to_vec()));
        let router = server_package_proxy_router(PackageProxyConfig::new(cetz_policy()), fetcher);

        let (status, headers, body) =
            proxy_get(router, "/typst/packages/preview/cetz-0.4.2.tar.gz").await;

        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(
            headers.get(axum::http::header::CONTENT_TYPE).unwrap(),
            PACKAGE_ARCHIVE_CONTENT_TYPE
        );
        assert_eq!(
            headers.get(axum::http::header::CACHE_CONTROL).unwrap(),
            PACKAGE_ARCHIVE_CACHE_CONTROL
        );
        assert_eq!(body, b"archive-bytes");
    }

    #[tokio::test]
    async fn proxy_denies_packages_outside_the_policy() {
        let (fetcher, calls) = StubFetcher::new(Some(b"archive-bytes".to_vec()));
        let router = server_package_proxy_router(PackageProxyConfig::new(cetz_policy()), fetcher);

        let (status, _, _) = proxy_get(router, "/typst/packages/preview/tablex-0.0.9.tar.gz").await;

        assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(*calls.lock().expect("test lock"), 0);
    }

    #[tokio::test]
    async fn proxy_rejects_invalid_archive_names() {
        for path in [
            "/typst/packages/preview/cetz-0.4.2.zip",
            "/typst/packages/preview/cetz.tar.gz",
            "/typst/packages/preview/cetz-latest.tar.gz",
            "/typst/packages/pre..view/cetz-0.4.2.tar.gz",
        ] {
            let (fetcher, calls) = StubFetcher::new(Some(b"archive-bytes".to_vec()));
            let router = server_package_proxy_router(
                PackageProxyConfig::new(PackagePolicy::allow_all()),
                fetcher,
            );

            let (status, _, _) = proxy_get(router, path).await;

            assert_eq!(
                status,
                axum::http::StatusCode::BAD_REQUEST,
                "path should be rejected: {path}"
            );
            assert_eq!(*calls.lock().expect("test lock"), 0);
        }
    }

    #[tokio::test]
    async fn proxy_maps_missing_upstream_archives_to_not_found() {
        let (fetcher, _) = StubFetcher::new(None);
        let router = server_package_proxy_router(PackageProxyConfig::new(cetz_policy()), fetcher);

        let (status, _, _) = proxy_get(router, "/typst/packages/preview/cetz-0.4.2.tar.gz").await;

        assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn proxy_serves_cached_archives_without_upstream_requests() {
        let dir = tempfile::tempdir().expect("tempdir should create");
        let (fetcher, calls) = StubFetcher::new(Some(b"archive-bytes".to_vec()));
        let router = server_package_proxy_router(
            PackageProxyConfig::new(cetz_policy()).with_cache_dir(dir.path()),
            fetcher,
        );

        let (status, _, _) =
            proxy_get(router.clone(), "/typst/packages/preview/cetz-0.4.2.tar.gz").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(*calls.lock().expect("test lock"), 1);

        let (status, _, body) =
            proxy_get(router, "/typst/packages/preview/cetz-0.4.2.tar.gz").await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, b"archive-bytes");
        assert_eq!(*calls.lock().expect("test lock"), 1);
    }
}
