use crate::registry::{Downloader, UNIVERSE_NAMESPACE, download_package_archive};
use crate::{PackageResolveError, PackageSpec};
use std::path::{Path, PathBuf};

/// Download verbatim `.tar.gz` package archives for later embedding.
///
/// Archives are written to `<dir>/<namespace>/<name>-<version>.tar.gz`, matching
/// [`package_archive_url`](crate::package_archive_url) so a vendored directory can also back a
/// static registry mirror. Existing files are kept as-is, making vendoring idempotent; the
/// written archives are meant to be embedded with `include_bytes!` and parsed by
/// [`PackageBundle::from_tar_gz`](crate::PackageBundle::from_tar_gz).
pub fn vendor_package_archives(
    downloader: &dyn Downloader,
    registry_url: &str,
    specs: &[PackageSpec],
    dir: &Path,
) -> Result<Vec<PathBuf>, VendorError> {
    let mut paths = Vec::new();

    for spec in specs {
        if spec.namespace != UNIVERSE_NAMESPACE {
            return Err(VendorError::UnsupportedNamespace { spec: spec.clone() });
        }

        let target = dir
            .join(spec.namespace.as_str())
            .join(format!("{}-{}.tar.gz", spec.name, spec.version));

        if target.is_file() {
            paths.push(target);
            continue;
        }

        let bytes = download_package_archive(downloader, registry_url, spec).map_err(|error| {
            VendorError::Download {
                spec: spec.clone(),
                message: match error {
                    PackageResolveError::NotFound { .. } => "not found".to_owned(),
                    PackageResolveError::Retrieval { message, .. } => message,
                    other => format!("{other:?}"),
                },
            }
        })?;

        let io_error = |error: std::io::Error| VendorError::Io {
            path: target.to_string_lossy().into_owned(),
            message: error.to_string(),
        };
        let parent = target
            .parent()
            .expect("vendored archive path always has a namespace parent");
        std::fs::create_dir_all(parent).map_err(io_error)?;
        std::fs::write(&target, &bytes).map_err(io_error)?;

        paths.push(target);
    }

    Ok(paths)
}

/// A package vendoring failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VendorError {
    /// The registry does not serve archives for the spec's namespace.
    UnsupportedNamespace { spec: PackageSpec },

    /// The archive could not be downloaded from the registry.
    Download { spec: PackageSpec, message: String },

    /// The archive could not be written to the vendor directory.
    Io { path: String, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackageBundle;
    use crate::registry::tests::{StubDownloader, tar_gz};

    fn spec(spec: &str) -> PackageSpec {
        spec.parse().expect("test spec should parse")
    }

    #[test]
    fn vendor_package_archives_writes_verbatim_archives() {
        let dir = tempfile::tempdir().expect("tempdir should create");
        let archive = tar_gz(&[("lib.typ", b"#let answer = 42".as_slice())]);
        let downloader = StubDownloader::new([(
            "https://registry.test/preview/example-0.1.0.tar.gz".to_owned(),
            archive.clone(),
        )]);
        let requests = downloader.requests();

        let paths = vendor_package_archives(
            &downloader,
            "https://registry.test",
            &[spec("@preview/example:0.1.0")],
            dir.path(),
        )
        .expect("vendoring should succeed");

        assert_eq!(paths, vec![dir.path().join("preview/example-0.1.0.tar.gz")]);
        assert_eq!(std::fs::read(&paths[0]).unwrap(), archive);

        // The written archive parses back into a Package Bundle for embedding.
        let bundle = PackageBundle::from_tar_gz(
            spec("@preview/example:0.1.0"),
            std::fs::read(&paths[0]).unwrap(),
        )
        .expect("vendored archive should parse");
        assert_eq!(
            bundle.file_bytes("lib.typ"),
            Some(b"#let answer = 42".as_slice())
        );

        // Vendoring again is idempotent and skips the download.
        vendor_package_archives(
            &downloader,
            "https://registry.test",
            &[spec("@preview/example:0.1.0")],
            dir.path(),
        )
        .expect("re-vendoring should succeed");
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn vendor_package_archives_reject_non_universe_namespaces() {
        let dir = tempfile::tempdir().expect("tempdir should create");
        let downloader = StubDownloader::new([]);

        let result = vendor_package_archives(
            &downloader,
            "https://registry.test",
            &[spec("@local/example:0.1.0")],
            dir.path(),
        );

        assert_eq!(
            result,
            Err(VendorError::UnsupportedNamespace {
                spec: spec("@local/example:0.1.0")
            })
        );
    }
}
