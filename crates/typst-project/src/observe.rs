//! Observed Package Dependency collection.
//!
//! One mechanism observes which packages Typst requested during a compile: wrap the
//! world, record file requests, and derive deduplicated, sorted Package Specs from the
//! package-rooted ones. The FileId identity helpers live here too so world routing and
//! Diagnostics mapping share one derivation of package-vs-project identity.

use crate::PackageSpec;
use std::sync::Mutex;
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, Source, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};

/// Return the workspace- or package-internal path for a Typst file id.
pub(crate) fn file_id_path(id: FileId) -> String {
    id.vpath().get_without_slash().to_owned()
}

/// Return the exact Package Spec a Typst file id is rooted in, if any.
pub(crate) fn file_id_package(id: FileId) -> Option<&'static PackageSpec> {
    match id.get().root() {
        VirtualRoot::Project => None,
        VirtualRoot::Package(package) => Some(package),
    }
}

/// Deduplicate and sort the Package Specs rooted in the given file ids.
///
/// The output is sorted by spec string so observations are deterministic regardless of
/// the order Typst requested files in.
pub(crate) fn package_specs_from_file_ids(
    ids: impl IntoIterator<Item = FileId>,
) -> Vec<PackageSpec> {
    let mut packages: Vec<PackageSpec> = Vec::new();

    for id in ids {
        let Some(package) = file_id_package(id) else {
            continue;
        };

        if !packages.iter().any(|existing| existing == package) {
            packages.push(package.clone());
        }
    }

    packages.sort_by_key(PackageSpec::to_string);
    packages
}

/// A Complete Typst World wrapper recording the file ids Typst requests.
pub(crate) struct RecordingWorld<'a> {
    base: &'a dyn World,
    requests: Mutex<Vec<FileId>>,
}

impl<'a> RecordingWorld<'a> {
    pub(crate) fn new(base: &'a dyn World) -> Self {
        Self {
            base,
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Return deduplicated, sorted Package Specs observed through file access.
    pub(crate) fn observed_packages(&self) -> Vec<PackageSpec> {
        let requests = self
            .requests
            .lock()
            .expect("observation recorder mutex should not be poisoned");

        package_specs_from_file_ids(requests.iter().copied())
    }

    fn record(&self, id: FileId) {
        self.requests
            .lock()
            .expect("observation recorder mutex should not be poisoned")
            .push(id);
    }
}

impl World for RecordingWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.base.library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.base.book()
    }

    fn main(&self) -> FileId {
        self.base.main()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.record(id);
        self.base.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.record(id);
        self.base.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.base.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.base.today(offset)
    }
}
