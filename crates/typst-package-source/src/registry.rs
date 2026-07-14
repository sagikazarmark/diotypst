use crate::source::{
    PackageResolveFuture, PackageSource, SyncPackageSource, UNIVERSE_REGISTRY_URL,
    package_archive_url,
};
use crate::{FsPackages, PackageBundle, PackageResolveError, PackageSpec};
use std::sync::Arc;
use typst_kit::packages::UniversePackages;

pub use typst_kit::downloader::{Downloader, Progress, ProgressDownloader, ProgressReporter};

/// The namespace the Typst Universe registry serves packages from.
pub const UNIVERSE_NAMESPACE: &str = typst_kit::packages::UniversePackages::NAMESPACE;

/// Download a verbatim `.tar.gz` package archive from a registry.
///
/// This fetches the raw registry bytes without unpacking them, so the same archive can back a
/// Package Bundle, an on-disk cache entry, a vendored file for embedding, or a proxy response.
pub fn download_package_archive(
    downloader: &dyn Downloader,
    registry_url: &str,
    spec: &PackageSpec,
) -> Result<Vec<u8>, PackageResolveError> {
    let url = package_archive_url(registry_url, spec);

    downloader
        .download(spec, &url)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => PackageResolveError::NotFound { spec: spec.clone() },
            _ => PackageResolveError::Retrieval {
                spec: spec.clone(),
                message: error.to_string(),
            },
        })
}

/// A Package Source downloading packages from a Typst Universe-style registry.
///
/// Downloads go through an explicit [`Downloader`]; nothing is fetched outside a `resolve`
/// call. An optional package directory retains downloads like the Typst CLI package cache.
pub struct RegistryPackages {
    downloader: SharedDownloader,
    universe: UniversePackages,
    url: String,
    cache: Option<FsPackages>,
}

/// A reference-counted [`Downloader`] handle, so one downloader can serve both the
/// verbatim archive downloads and typst-kit's package-index lookups.
#[derive(Clone)]
struct SharedDownloader(Arc<dyn Downloader>);

impl Downloader for SharedDownloader {
    fn stream(
        &self,
        key: &dyn std::any::Any,
        url: &str,
    ) -> std::io::Result<(Option<usize>, Box<dyn std::io::Read>)> {
        self.0.stream(key, url)
    }
}

impl RegistryPackages {
    /// Download packages from the official Typst Universe registry.
    pub fn new(downloader: impl Downloader) -> Self {
        Self::with_url(downloader, UNIVERSE_REGISTRY_URL)
    }

    /// Download packages from a registry mirror.
    pub fn with_url(downloader: impl Downloader, url: impl Into<String>) -> Self {
        let url = url.into();
        let downloader = SharedDownloader(Arc::new(downloader));

        Self {
            universe: UniversePackages::with_url(downloader.clone(), url.clone()),
            downloader,
            url,
            cache: None,
        }
    }

    /// Retain downloaded packages unpacked in a package directory.
    ///
    /// Cached packages are served from the directory without a network request on later
    /// resolutions, matching the Typst CLI package cache behavior.
    pub fn cache(mut self, cache: FsPackages) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Return the registry URL packages are downloaded from.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl SyncPackageSource for RegistryPackages {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        if spec.namespace != UNIVERSE_NAMESPACE {
            return Err(PackageResolveError::NotFound { spec: spec.clone() });
        }

        if let Some(cache) = &self.cache {
            match cache.resolve_sync(spec) {
                Ok(bundle) => return Ok(bundle),
                Err(PackageResolveError::NotFound { .. }) => {}
                Err(error) => return Err(error),
            }
        }

        let bytes = download_package_archive(&self.downloader, &self.url, spec).map_err(
            |error| match error {
                // Distinguish a missing version from a missing package, like typst-cli:
                // the package index knows the latest version if the package exists at all.
                PackageResolveError::NotFound { spec } => {
                    match self.universe.latest_version(&spec.versionless()) {
                        Ok(latest) => PackageResolveError::VersionNotFound { spec, latest },
                        Err(_) => PackageResolveError::NotFound { spec },
                    }
                }
                other => other,
            },
        )?;
        let bundle = PackageBundle::from_tar_gz(spec.clone(), &bytes).map_err(|error| {
            PackageResolveError::Malformed {
                spec: spec.clone(),
                message: format!("{error:?}"),
            }
        })?;

        if let Some(cache) = &self.cache {
            let decoder = flate2::read::GzDecoder::new(bytes.as_slice());
            let mut archive = tar::Archive::new(decoder);

            cache
                .store(spec, |dir| {
                    archive.unpack(dir).map_err(|error| {
                        typst_library::diag::PackageError::MalformedArchive(Some(
                            error.to_string().into(),
                        ))
                    })
                })
                .map_err(|error| PackageResolveError::Retrieval {
                    spec: spec.clone(),
                    message: error.to_string(),
                })?;
        }

        Ok(bundle)
    }
}

impl PackageSource for RegistryPackages {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        Box::pin(std::future::ready(self.resolve_sync(spec)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::any::Any;
    use std::collections::HashMap;
    use std::io::Read;
    use std::sync::{Arc, Mutex};

    fn spec(spec: &str) -> PackageSpec {
        spec.parse().expect("test spec should parse")
    }

    pub(crate) fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);

        for (path, bytes) in entries {
            let mut header = tar::Header::new_ustar();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, path, *bytes)
                .expect("test archive entry should append");
        }

        builder
            .into_inner()
            .expect("test archive should finish")
            .finish()
            .expect("test archive should compress")
    }

    /// A downloader serving fixture bytes by URL, recording requests in a shared log.
    pub(crate) struct StubDownloader {
        responses: HashMap<String, Vec<u8>>,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl StubDownloader {
        pub(crate) fn new(responses: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
            Self {
                responses: responses.into_iter().collect(),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub(crate) fn requests(&self) -> Arc<Mutex<Vec<String>>> {
            Arc::clone(&self.requests)
        }
    }

    impl Downloader for StubDownloader {
        fn stream(
            &self,
            _key: &dyn Any,
            url: &str,
        ) -> std::io::Result<(Option<usize>, Box<dyn Read>)> {
            self.requests.lock().unwrap().push(url.to_owned());

            match self.responses.get(url) {
                Some(bytes) => Ok((
                    Some(bytes.len()),
                    Box::new(std::io::Cursor::new(bytes.clone())),
                )),
                None => Err(std::io::ErrorKind::NotFound.into()),
            }
        }
    }

    #[test]
    fn registry_packages_download_and_parse_archives() {
        let archive = tar_gz(&[("lib.typ", b"#let answer = 42".as_slice())]);
        let source = RegistryPackages::with_url(
            StubDownloader::new([(
                "https://registry.test/preview/example-0.1.0.tar.gz".to_owned(),
                archive,
            )]),
            "https://registry.test",
        );

        let bundle = source
            .resolve_sync(&spec("@preview/example:0.1.0"))
            .expect("package should resolve");
        assert_eq!(
            bundle.file_bytes("lib.typ"),
            Some(b"#let answer = 42".as_slice())
        );

        assert_eq!(
            source.resolve_sync(&spec("@preview/missing:0.1.0")),
            Err(PackageResolveError::NotFound {
                spec: spec("@preview/missing:0.1.0")
            })
        );
    }

    #[test]
    fn registry_packages_skip_non_universe_namespaces() {
        let downloader = StubDownloader::new([]);
        let requests = downloader.requests();
        let source = RegistryPackages::with_url(downloader, "https://registry.test");

        assert_eq!(
            source.resolve_sync(&spec("@local/example:0.1.0")),
            Err(PackageResolveError::NotFound {
                spec: spec("@local/example:0.1.0")
            })
        );
        assert!(requests.lock().unwrap().is_empty());
    }

    #[test]
    fn registry_packages_report_missing_versions_with_the_latest_known_version() {
        // The archive 404s, but the package index knows the package: the failure is a
        // VersionNotFound carrying the latest version, matching typst-cli's UX.
        let index = br#"[{"name": "example", "version": "0.4.2"}]"#.to_vec();
        let source = RegistryPackages::with_url(
            StubDownloader::new([("https://registry.test/preview/index.json".to_owned(), index)]),
            "https://registry.test",
        );

        assert_eq!(
            source.resolve_sync(&spec("@preview/example:9.9.9")),
            Err(PackageResolveError::VersionNotFound {
                spec: spec("@preview/example:9.9.9"),
                latest: "0.4.2".parse().expect("version should parse"),
            })
        );

        // A package the index has never heard of stays a plain NotFound.
        assert_eq!(
            source.resolve_sync(&spec("@preview/unknown:1.0.0")),
            Err(PackageResolveError::NotFound {
                spec: spec("@preview/unknown:1.0.0")
            })
        );
    }

    #[test]
    fn registry_packages_retain_downloads_in_cache() {
        let dir = tempfile::tempdir().expect("tempdir should create");
        let archive = tar_gz(&[("lib.typ", b"#let answer = 42".as_slice())]);
        let downloader = StubDownloader::new([(
            "https://registry.test/preview/example-0.1.0.tar.gz".to_owned(),
            archive,
        )]);
        let requests = downloader.requests();
        let source = RegistryPackages::with_url(downloader, "https://registry.test")
            .cache(FsPackages::new(dir.path()));

        source
            .resolve_sync(&spec("@preview/example:0.1.0"))
            .expect("first resolution should download");
        assert_eq!(requests.lock().unwrap().len(), 1);

        source
            .resolve_sync(&spec("@preview/example:0.1.0"))
            .expect("second resolution should hit the cache");
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
}
