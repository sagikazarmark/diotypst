//! Browser-side package downloading for World Preparation.

/// Base path served by the optional server-side package proxy.
///
/// The proxy serves verbatim `.tar.gz` archives at
/// `{base}/{namespace}/{name}-{version}.tar.gz`, mirroring the Typst Universe registry layout.
pub const SERVER_PACKAGE_PROXY_BASE: &str = "/typst/packages";

#[cfg(all(target_arch = "wasm32", feature = "archive"))]
mod fetch {
    use super::SERVER_PACKAGE_PROXY_BASE;
    use libtypst::{
        PackageBundle, PackagePolicy, PackageResolveError, PackageResolveFuture, PackageSource,
        PackageSpec, UNIVERSE_REGISTRY_URL, package_archive_url,
    };
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    /// A Package Source downloading verbatim `.tar.gz` archives with the browser `fetch` API.
    ///
    /// Downloads happen during World Preparation only; rendering never fetches. The optional
    /// Package Policy rejects denied specs before any request as a fast path; a server-side
    /// proxy policy stays authoritative.
    pub struct FetchPackageSource {
        base_url: String,
        policy: Option<PackagePolicy>,
    }

    impl FetchPackageSource {
        /// Download through the same-origin server package proxy.
        ///
        /// This pairs with `server_package_proxy_router` on a fullstack server and works
        /// regardless of registry CORS headers.
        pub fn proxy() -> Self {
            Self::with_base_url(SERVER_PACKAGE_PROXY_BASE)
        }

        /// Download directly from the official Typst Universe registry.
        ///
        /// Direct browser downloads require the registry to allow cross-origin requests; use
        /// [`FetchPackageSource::proxy`] when serving through a fullstack server.
        pub fn universe() -> Self {
            Self::with_base_url(UNIVERSE_REGISTRY_URL)
        }

        /// Download from an explicit registry or proxy base URL.
        pub fn with_base_url(base_url: impl Into<String>) -> Self {
            Self {
                base_url: base_url.into(),
                policy: None,
            }
        }

        /// Reject specs denied by a Package Policy before requesting them.
        pub fn with_policy(mut self, policy: PackagePolicy) -> Self {
            self.policy = Some(policy);
            self
        }

        /// Return the registry or proxy base URL archives are fetched from.
        pub fn base_url(&self) -> &str {
            &self.base_url
        }

        async fn fetch_bundle(
            &self,
            spec: &PackageSpec,
        ) -> Result<PackageBundle, PackageResolveError> {
            let url = package_archive_url(&self.base_url, spec);
            let bytes = fetch_bytes(&url, spec).await?;

            PackageBundle::from_tar_gz(spec.clone(), &bytes).map_err(|error| {
                PackageResolveError::Malformed {
                    spec: spec.clone(),
                    message: format!("{error:?}"),
                }
            })
        }
    }

    impl PackageSource for FetchPackageSource {
        fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
            if let Some(policy) = &self.policy {
                if !policy.permits(spec) {
                    return Box::pin(std::future::ready(Err(PackageResolveError::Denied {
                        spec: spec.clone(),
                    })));
                }
            }

            Box::pin(self.fetch_bundle(spec))
        }
    }

    async fn fetch_bytes(url: &str, spec: &PackageSpec) -> Result<Vec<u8>, PackageResolveError> {
        let retrieval = |message: String| PackageResolveError::Retrieval {
            spec: spec.clone(),
            message,
        };

        let window = web_sys::window()
            .ok_or_else(|| retrieval("browser window is unavailable".to_owned()))?;
        let response = JsFuture::from(window.fetch_with_str(url))
            .await
            .map_err(|error| retrieval(format!("fetch failed: {error:?}")))?;
        let response: web_sys::Response = response
            .dyn_into()
            .map_err(|_| retrieval("fetch did not return a Response".to_owned()))?;

        match response.status() {
            200..=299 => {}
            404 => return Err(PackageResolveError::NotFound { spec: spec.clone() }),
            403 => return Err(PackageResolveError::Denied { spec: spec.clone() }),
            status => return Err(retrieval(format!("registry responded with HTTP {status}"))),
        }

        let buffer = JsFuture::from(
            response
                .array_buffer()
                .map_err(|error| retrieval(format!("reading response failed: {error:?}")))?,
        )
        .await
        .map_err(|error| retrieval(format!("reading response failed: {error:?}")))?;

        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }
}

#[cfg(all(target_arch = "wasm32", feature = "archive"))]
pub use fetch::FetchPackageSource;
