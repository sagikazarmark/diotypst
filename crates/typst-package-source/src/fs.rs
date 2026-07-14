//! Package Source implementations for typst-kit's [`FsPackages`] and [`SystemPackages`].
//!
//! [`FsPackages`] serves unpacked packages from one Typst CLI-style package
//! directory (`namespace/name/version` subdirectories); [`SystemPackages`] is
//! typst-kit's full CLI resolution chain — data directory, then cache directory,
//! then a Typst Universe download stored into the cache. Resolving either as a
//! Package Source eagerly reads the package's files into an in-memory
//! [`PackageBundle`].

use crate::source::{PackageResolveFuture, PackageSource, SyncPackageSource};
use crate::{PackageBundle, PackageResolveError, PackageSpec};
use std::path::Path;
use typst_kit::packages::{FsPackages, SystemPackages};

impl SyncPackageSource for FsPackages {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        let root = self
            .obtain(spec)
            .ok_or_else(|| PackageResolveError::NotFound { spec: spec.clone() })?;
        read_package_dir(spec, root.path())
    }
}

impl PackageSource for FsPackages {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        Box::pin(std::future::ready(self.resolve_sync(spec)))
    }
}

impl SyncPackageSource for SystemPackages {
    fn resolve_sync(&self, spec: &PackageSpec) -> Result<PackageBundle, PackageResolveError> {
        use typst_library::diag::PackageError;

        let root = self.obtain(spec).map_err(|error| match error {
            PackageError::NotFound(spec) => PackageResolveError::NotFound { spec },
            PackageError::VersionNotFound(spec, latest) => {
                PackageResolveError::VersionNotFound { spec, latest }
            }
            PackageError::MalformedArchive(message) => PackageResolveError::Malformed {
                spec: spec.clone(),
                message: message
                    .map(|message| message.to_string())
                    .unwrap_or_default(),
            },
            other => PackageResolveError::Retrieval {
                spec: spec.clone(),
                message: other.to_string(),
            },
        })?;

        read_package_dir(spec, root.path())
    }
}

impl PackageSource for SystemPackages {
    fn resolve<'a>(&'a self, spec: &'a PackageSpec) -> PackageResolveFuture<'a> {
        Box::pin(std::future::ready(self.resolve_sync(spec)))
    }
}

/// Recursively collect package files below `root` as root-relative slash paths.
/// Read an unpacked package directory eagerly into an in-memory Package Bundle.
fn read_package_dir(spec: &PackageSpec, dir: &Path) -> Result<PackageBundle, PackageResolveError> {
    let mut builder = PackageBundle::builder(spec.clone());
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files).map_err(|error| PackageResolveError::Retrieval {
        spec: spec.clone(),
        message: error.to_string(),
    })?;

    for (path, bytes) in files {
        builder = builder.file(path, bytes);
    }

    builder
        .build()
        .map_err(|error| PackageResolveError::Malformed {
            spec: spec.clone(),
            message: format!("{error:?}"),
        })
}

fn collect_files(
    root: &Path,
    dir: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if entry.file_type()?.is_dir() {
            collect_files(root, &path, files)?;
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .expect("directory walk should stay below its root")
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        files.push((relative, std::fs::read(&path)?));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(spec: &str) -> PackageSpec {
        spec.parse().expect("test spec should parse")
    }

    #[test]
    fn fs_packages_resolve_unpacked_package_directories() {
        let dir = tempfile::tempdir().expect("tempdir should create");
        let package_dir = dir.path().join("preview/example/0.1.0/src");
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            dir.path().join("preview/example/0.1.0/typst.toml"),
            "[package]",
        )
        .unwrap();
        std::fs::write(package_dir.join("lib.typ"), "#let answer = 42").unwrap();

        let source = FsPackages::new(dir.path());

        let bundle = source
            .resolve_sync(&spec("@preview/example:0.1.0"))
            .expect("package should resolve");
        assert_eq!(
            bundle.file_bytes("typst.toml"),
            Some(b"[package]".as_slice())
        );
        assert_eq!(
            bundle.file_bytes("src/lib.typ"),
            Some(b"#let answer = 42".as_slice())
        );

        assert_eq!(
            source.resolve_sync(&spec("@preview/example:0.2.0")),
            Err(PackageResolveError::NotFound {
                spec: spec("@preview/example:0.2.0")
            })
        );
    }

    #[test]
    fn system_packages_resolve_through_the_cli_chain() {
        let data_dir = tempfile::tempdir().expect("tempdir should create");
        std::fs::create_dir_all(data_dir.path().join("local/example/0.1.0")).unwrap();
        std::fs::write(
            data_dir.path().join("local/example/0.1.0/lib.typ"),
            "#let answer = 42",
        )
        .unwrap();

        // No cache, no downloader use: the data directory satisfies the chain's
        // first step, exactly like the Typst CLI's data-dir packages.
        let source = SystemPackages::from_parts(
            Some(FsPackages::new(data_dir.path())),
            None,
            typst_kit::packages::UniversePackages::new(FailingDownloader),
        );

        let bundle = source
            .resolve_sync(&spec("@local/example:0.1.0"))
            .expect("data-dir package should resolve");
        assert_eq!(
            bundle.file_bytes("lib.typ"),
            Some(b"#let answer = 42".as_slice())
        );

        assert!(matches!(
            source.resolve_sync(&spec("@local/missing:0.1.0")),
            Err(PackageResolveError::NotFound { .. })
        ));
    }

    /// A downloader that fails every request, proving no ambient network is used.
    struct FailingDownloader;

    impl crate::Downloader for FailingDownloader {
        fn stream(
            &self,
            _key: &dyn std::any::Any,
            _url: &str,
        ) -> std::io::Result<(Option<usize>, Box<dyn std::io::Read>)> {
            Err(std::io::ErrorKind::NotFound.into())
        }
    }
}
