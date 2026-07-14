use crate::source::package_archive_url;
use crate::{PackagePolicy, PackageSpec};
use std::str::FromStr;

/// Content type every package proxy adapter sets on a served archive.
pub const PACKAGE_ARCHIVE_CONTENT_TYPE: &str = "application/gzip";

/// Cache-control every package proxy adapter sets on a served archive.
///
/// Exact package versions are immutable in Typst Universe.
pub const PACKAGE_ARCHIVE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// A parsed package proxy archive request: the transport-free core of a package proxy.
///
/// Adapters (an axum router, a fetch-based handler on a serverless host) do the IO
/// between these steps: parse the path, check the Package Policy, fetch the upstream
/// URL, and serve the bytes with the archive header constants. Everything
/// security-relevant — spec validation, policy enforcement, URL construction — lives
/// here, once.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyArchiveRequest {
    spec: PackageSpec,
}

impl ProxyArchiveRequest {
    /// Parse `{namespace}` and `{name}-{version}.tar.gz` into an exact Package Spec.
    ///
    /// Only spec components that survive Typst's own package spec validation ever reach
    /// an upstream URL or a cache path, which rules out traversal and request smuggling.
    pub fn parse(namespace: &str, archive: &str) -> Result<Self, PackageProxyError> {
        let invalid = || PackageProxyError::InvalidArchiveName(format!("{namespace}/{archive}"));

        let stem = archive.strip_suffix(".tar.gz").ok_or_else(invalid)?;
        // Package names may contain hyphens; versions never do, so split at the last one.
        let (name, version) = stem.rsplit_once('-').ok_or_else(invalid)?;

        let spec = PackageSpec::from_str(&format!("@{namespace}/{name}:{version}"))
            .map_err(|_| invalid())?;

        Ok(Self { spec })
    }

    /// Return the exact Package Spec this request asks for.
    pub fn spec(&self) -> &PackageSpec {
        &self.spec
    }

    /// Enforce a Package Policy before any network or disk access.
    pub fn permit(&self, policy: &PackagePolicy) -> Result<(), PackageProxyError> {
        if policy.permits(&self.spec) {
            Ok(())
        } else {
            Err(PackageProxyError::Denied(self.spec.clone()))
        }
    }

    /// Return the upstream registry URL serving this archive.
    pub fn upstream_url(&self, registry_base_url: &str) -> String {
        package_archive_url(registry_base_url, &self.spec)
    }

    /// Return the canonical `{name}-{version}.tar.gz` archive filename.
    pub fn archive_filename(&self) -> String {
        format!("{}-{}.tar.gz", self.spec.name, self.spec.version)
    }
}

/// A package proxy failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageProxyError {
    /// The request path was not a `{name}-{version}.tar.gz` archive of an exact Package Spec.
    InvalidArchiveName(String),

    /// The proxy's Package Policy denies this package.
    Denied(PackageSpec),

    /// The upstream registry does not serve this archive.
    UpstreamNotFound(PackageSpec),

    /// The upstream fetch failed.
    Upstream(String),
}

impl PackageProxyError {
    /// Return the transport-agnostic HTTP status code for this failure.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::InvalidArchiveName(_) => 400,
            Self::Denied(_) => 403,
            Self::UpstreamNotFound(_) => 404,
            Self::Upstream(_) => 502,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackagePattern;

    #[test]
    fn proxy_requests_parse_only_exact_archive_names() {
        let request = ProxyArchiveRequest::parse("preview", "cetz-core-0.1.2.tar.gz")
            .expect("hyphenated package names should parse");
        assert_eq!(request.spec().to_string(), "@preview/cetz-core:0.1.2");
        assert_eq!(request.archive_filename(), "cetz-core-0.1.2.tar.gz");

        for (namespace, archive) in [
            ("preview", "cetz-0.4.2.zip"),
            ("preview", "cetz.tar.gz"),
            ("preview", "cetz-latest.tar.gz"),
            ("pre..view", "cetz-0.4.2.tar.gz"),
            ("preview", "cetz-0.4.tar.gz"),
        ] {
            assert!(
                ProxyArchiveRequest::parse(namespace, archive).is_err(),
                "should reject: {namespace}/{archive}"
            );
        }
    }

    #[test]
    fn proxy_requests_enforce_the_package_policy() {
        let request = ProxyArchiveRequest::parse("preview", "cetz-0.4.2.tar.gz")
            .expect("archive name should parse");
        let policy = PackagePolicy::deny_all().allow(
            "@preview/cetz"
                .parse::<PackagePattern>()
                .expect("pattern should parse"),
        );

        assert_eq!(request.permit(&policy), Ok(()));

        let denied = ProxyArchiveRequest::parse("preview", "tablex-0.0.9.tar.gz")
            .expect("archive name should parse");
        assert!(matches!(
            denied.permit(&policy),
            Err(PackageProxyError::Denied(_))
        ));
    }

    #[test]
    fn proxy_requests_build_upstream_registry_urls() {
        let request = ProxyArchiveRequest::parse("preview", "cetz-0.4.2.tar.gz")
            .expect("archive name should parse");

        assert_eq!(
            request.upstream_url("https://packages.typst.org"),
            "https://packages.typst.org/preview/cetz-0.4.2.tar.gz"
        );
    }
}
