use crate::{PackageBundle, PackageBundleError, PackageSpec};
use std::io::Read;

impl PackageBundle {
    /// Parse a verbatim Typst Universe `.tar.gz` package archive into a Package Bundle.
    ///
    /// This accepts the archives served by the Typst Universe registry unchanged, so
    /// pre-downloaded archives can be embedded into a binary with `include_bytes!` and turned
    /// into Package Bundles at startup. Only regular file entries contribute to the bundle.
    pub fn from_tar_gz(
        spec: PackageSpec,
        bytes: impl AsRef<[u8]>,
    ) -> Result<PackageBundle, PackageArchiveError> {
        let decoder = flate2::read::GzDecoder::new(bytes.as_ref());
        let mut archive = tar::Archive::new(decoder);
        let mut builder = PackageBundle::builder(spec);

        let entries = archive.entries().map_err(PackageArchiveError::from_io)?;
        for entry in entries {
            let mut entry = entry.map_err(PackageArchiveError::from_io)?;

            if !entry.header().entry_type().is_file() {
                continue;
            }

            let path = entry
                .path()
                .map_err(PackageArchiveError::from_io)?
                .to_string_lossy()
                .into_owned();
            let mut file_bytes = Vec::new();
            entry
                .read_to_end(&mut file_bytes)
                .map_err(PackageArchiveError::from_io)?;

            builder = builder.file(path, file_bytes);
        }

        builder.build().map_err(|error| match error {
            PackageBundleError::InvalidPath { path } => PackageArchiveError::InvalidPath { path },
            PackageBundleError::DuplicatePath { path } => {
                PackageArchiveError::DuplicatePath { path }
            }
        })
    }
}

/// A package archive parsing failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageArchiveError {
    /// The bytes could not be read as a gzip-compressed tar archive.
    Archive { message: String },

    /// An archive entry path is not root-relative inside the package.
    InvalidPath { path: String },

    /// More than one archive entry has the same package-internal path.
    DuplicatePath { path: String },
}

impl PackageArchiveError {
    fn from_io(error: std::io::Error) -> Self {
        Self::Archive {
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(spec: &str) -> PackageSpec {
        spec.parse().expect("test spec should parse")
    }

    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
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

    #[test]
    fn from_tar_gz_round_trips_package_files() {
        let archive = tar_gz(&[
            ("typst.toml", b"[package]".as_slice()),
            ("src/lib.typ", b"#let answer = 42".as_slice()),
        ]);

        let bundle = PackageBundle::from_tar_gz(spec("@preview/example:0.1.0"), &archive)
            .expect("archive should parse");

        assert_eq!(bundle.spec(), &spec("@preview/example:0.1.0"));
        assert_eq!(
            bundle.file_bytes("typst.toml"),
            Some(b"[package]".as_slice())
        );
        assert_eq!(
            bundle.file_bytes("src/lib.typ"),
            Some(b"#let answer = 42".as_slice())
        );
    }

    #[test]
    fn from_tar_gz_rejects_escaping_paths() {
        // `tar::Builder` refuses to create `..` paths, so write the header name directly to
        // simulate a hostile archive.
        let name = "../evil.typ";
        let data = b"evil";
        let mut header = tar::Header::new_ustar();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_cksum();

        let mut tar_bytes = Vec::new();
        tar_bytes.extend_from_slice(header.as_bytes());
        tar_bytes.extend_from_slice(data);
        tar_bytes.resize(tar_bytes.len().div_ceil(512) * 512, 0);
        tar_bytes.extend_from_slice(&[0u8; 1024]);

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_bytes).unwrap();
        let archive = encoder.finish().unwrap();

        let result = PackageBundle::from_tar_gz(spec("@preview/example:0.1.0"), &archive);

        assert_eq!(
            result,
            Err(PackageArchiveError::InvalidPath {
                path: "../evil.typ".to_owned()
            })
        );
    }

    #[test]
    fn from_tar_gz_rejects_duplicate_paths() {
        let archive = tar_gz(&[
            ("lib.typ", b"one".as_slice()),
            ("lib.typ", b"two".as_slice()),
        ]);

        let result = PackageBundle::from_tar_gz(spec("@preview/example:0.1.0"), &archive);

        assert_eq!(
            result,
            Err(PackageArchiveError::DuplicatePath {
                path: "lib.typ".to_owned()
            })
        );
    }

    #[test]
    fn from_tar_gz_rejects_garbage_bytes() {
        let result = PackageBundle::from_tar_gz(spec("@preview/example:0.1.0"), b"not an archive");

        assert!(matches!(result, Err(PackageArchiveError::Archive { .. })));
    }
}
