use typst_syntax::VirtualPath;

/// Parse a Typst path that can name a file: any valid [`VirtualPath`] except the bare
/// root, which cannot.
///
/// Rooted and root-relative spellings normalize to the same path; escaping paths are
/// rejected. This is the shared validation rule for Project Paths and package-internal
/// paths.
pub fn parse_file_path(path: &str) -> Option<VirtualPath> {
    match VirtualPath::new(path) {
        Ok(vpath) if !vpath.is_root() => Some(vpath),
        _ => None,
    }
}
