//! A native HTTPS [`Downloader`] built on rustls.
//!
//! This is the OpenSSL-free counterpart to typst-kit's `SystemDownloader`. TLS
//! is pure Rust, so the build links no OpenSSL and no system TLS library, and
//! needs no pkg-config or libssl-dev. Trust roots still come from the platform
//! certificate store, so a private CA installed system-wide keeps working.
//!
//! Note that rustls's `ring` backend still compiles a small amount of C and
//! assembly through `cc`, so a C compiler is required even though no C TLS
//! library is linked.

use std::any::Any;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;

use crate::registry::Downloader;

/// A trust anchor could not be established.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RustlsCertError {
    /// The configured certificate file could not be read.
    #[error("certificate file {path} could not be read: {message}")]
    Read {
        /// The configured certificate path.
        path: PathBuf,
        /// What the read reported.
        message: String,
    },

    /// The configured file held no PEM certificate.
    #[error("certificate file {path} contains no PEM certificate")]
    Parse {
        /// The configured certificate path.
        path: PathBuf,
    },

    /// A configured certificate parsed as PEM but is not a usable trust anchor.
    ///
    /// PEM framing only base64-decodes; this is the DER rejecting it.
    #[error("certificate file {path} is not a usable trust anchor: {message}")]
    Rejected {
        /// The configured certificate path.
        path: PathBuf,
        /// Why rustls refused the certificate.
        message: String,
    },

    /// No trust anchor is available, so every connection would fail.
    #[error("no trust anchors are available: {message}")]
    NoTrustAnchors {
        /// What the platform certificate store reported, if anything.
        message: String,
    },
}

/// A minimal HTTPS client for package downloads, with rustls for TLS.
///
/// Trust roots come from the platform certificate store. An optional custom
/// PEM certificate is added on top, for a private registry whose CA is not
/// installed system-wide. Proxy settings are read from the usual environment
/// variables, matching typst-kit's `SystemDownloader`.
///
/// A registry that sends headers and then stalls would otherwise hold a
/// download open forever, so connect and per-read stalls are bounded. The read
/// bound applies to one read of the body rather than the whole transfer, so a
/// slow but progressing download of a large package is not cut short.
///
/// The TLS configuration is built once, on the first download, and reused.
///
/// # Examples
///
/// Back a registry Package Source with rustls instead of the OS TLS stack:
///
/// ```
/// use typst_package_source::{RegistryPackages, RustlsDownloader};
///
/// let packages = RegistryPackages::new(RustlsDownloader::new(concat!(
///     env!("CARGO_PKG_NAME"),
///     "/",
///     env!("CARGO_PKG_VERSION"),
/// )));
///
/// assert_eq!(packages.url(), typst_package_source::UNIVERSE_REGISTRY_URL);
/// ```
///
/// Trust a private registry's CA that is not installed system-wide:
///
/// ```
/// use typst_package_source::{RegistryPackages, RustlsDownloader};
///
/// let downloader = RustlsDownloader::with_cert_path("my-app/1.0", "/etc/my-app/registry-ca.pem");
/// let packages = RegistryPackages::with_url(downloader, "https://packages.example.com");
///
/// assert_eq!(packages.url(), "https://packages.example.com");
/// ```
pub struct RustlsDownloader {
    user_agent: String,
    cert_path: Option<PathBuf>,
    connect_timeout: Duration,
    read_timeout: Duration,
    tls: OnceLock<Arc<ClientConfig>>,
}

/// How long establishing a connection may take before the download fails.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long one read of a response may stall before the download fails.
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(30);

impl RustlsDownloader {
    /// Creates a downloader trusting the platform certificate store.
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self {
            user_agent: user_agent.into(),
            cert_path: None,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            tls: OnceLock::new(),
        }
    }

    /// Creates a downloader that also trusts the PEM certificate at `cert_path`.
    ///
    /// The file is read on the first download. A certificate that cannot be
    /// read, parsed, or accepted as a trust anchor fails that download rather
    /// than being silently dropped, so a misconfigured trust anchor cannot look
    /// like a plain connection failure.
    pub fn with_cert_path(user_agent: impl Into<String>, cert_path: impl Into<PathBuf>) -> Self {
        Self {
            user_agent: user_agent.into(),
            cert_path: Some(cert_path.into()),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            tls: OnceLock::new(),
        }
    }

    /// Bounds how long establishing a connection may take.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Bounds how long one read of a response may stall.
    ///
    /// This is not a ceiling on the whole transfer: a large package that keeps
    /// arriving is never cut short, while a registry that goes silent fails.
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = timeout;
        self
    }

    /// Collects the trust anchors this downloader accepts.
    ///
    /// The platform store is best-effort: a store that yields nothing is not
    /// fatal on its own, since a configured certificate may be the only anchor
    /// a deployment needs. A configured certificate that cannot be read, parsed,
    /// or accepted is always fatal, and so is ending up with no anchor at all.
    fn trust_roots(&self) -> Result<RootCertStore, RustlsCertError> {
        let mut roots = RootCertStore::empty();

        // rustls-native-certs reads the platform store without OpenSSL, and
        // reports per-certificate failures rather than losing them.
        let platform = rustls_native_certs::load_native_certs();
        roots.add_parsable_certificates(platform.certs);

        if let Some(path) = &self.cert_path {
            for cert in read_pem_certificates(path)? {
                roots.add(cert).map_err(|error| RustlsCertError::Rejected {
                    path: path.clone(),
                    message: error.to_string(),
                })?;
            }
        }

        if roots.is_empty() {
            return Err(RustlsCertError::NoTrustAnchors {
                message: if platform.errors.is_empty() {
                    "the platform certificate store is empty".to_owned()
                } else {
                    platform
                        .errors
                        .iter()
                        .map(|error| error.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                },
            });
        }

        Ok(roots)
    }

    /// Returns the shared TLS configuration, building it on first use.
    fn tls_config(&self) -> Result<&Arc<ClientConfig>, RustlsCertError> {
        if let Some(config) = self.tls.get() {
            return Ok(config);
        }

        let roots = self.trust_roots()?;
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .expect("ring supports the default protocol versions")
                .with_root_certificates(roots)
                .with_no_client_auth();

        Ok(self.tls.get_or_init(|| Arc::new(config)))
    }
}

impl Downloader for RustlsDownloader {
    fn stream(&self, _: &dyn Any, url: &str) -> io::Result<(Option<usize>, Box<dyn Read>)> {
        let mut builder = ureq::AgentBuilder::new()
            .user_agent(&self.user_agent)
            .timeout_connect(self.connect_timeout)
            .timeout_read(self.read_timeout)
            .timeout_write(self.read_timeout)
            .tls_config(self.tls_config().map_err(io::Error::other)?.clone());

        // Honor the ambient proxy configuration, as the Typst CLI does.
        if let Some(proxy) = env_proxy::for_url_str(url)
            .to_url()
            .and_then(|url| ureq::Proxy::new(url).ok())
        {
            builder = builder.proxy(proxy);
        }

        let response = builder
            .build()
            .get(url)
            .call()
            .map_err(|error| match error {
                // The registry reports a missing package as 404; the Package Source
                // maps this kind onto `PackageResolveError::NotFound`.
                ureq::Error::Status(404, _) => io::Error::new(io::ErrorKind::NotFound, error),
                error => io::Error::other(error),
            })?;

        let content_length = response
            .header("Content-Length")
            .and_then(|header| header.parse().ok());

        Ok((content_length, response.into_reader()))
    }
}

impl std::fmt::Debug for RustlsDownloader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RustlsDownloader")
            .field("user_agent", &self.user_agent)
            .field("cert_path", &self.cert_path)
            .field("connect_timeout", &self.connect_timeout)
            .field("read_timeout", &self.read_timeout)
            .finish_non_exhaustive()
    }
}

/// Reads every PEM certificate in a file, rejecting one that holds none.
fn read_pem_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, RustlsCertError> {
    let certificates = CertificateDer::pem_file_iter(path)
        .map_err(|error| pem_error(path, error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| pem_error(path, error))?;

    if certificates.is_empty() {
        return Err(RustlsCertError::Parse {
            path: path.to_path_buf(),
        });
    }

    Ok(certificates)
}

fn pem_error(path: &Path, error: rustls_pki_types::pem::Error) -> RustlsCertError {
    match error {
        rustls_pki_types::pem::Error::Io(error) => RustlsCertError::Read {
            path: path.to_path_buf(),
            message: error.to_string(),
        },
        _ => RustlsCertError::Parse {
            path: path.to_path_buf(),
        },
    }
}

#[cfg(test)]
mod tests;
