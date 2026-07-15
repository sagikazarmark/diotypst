//! Small, focused example components: one per feature area.
//!
//! Each component keeps the `diotypst` API front and center. Project-agnostic
//! presentation lives in [`crate::components`], while Typst-specific
//! presentation shared by examples stays in this module. The `pages` module
//! mounts these live *and* renders their source with the compile-time `code!`
//! macro, guaranteeing the code shown is the code that runs.

mod presentation;

pub(crate) use presentation::{PreparationPackageList, TypstPreview};

pub mod editor;
pub mod embedded;
pub mod import;
pub mod json_inputs;
pub mod minimal;
pub mod multi_file;
pub mod pack;
pub mod page_images;
pub mod pdf_download;
pub mod policy;
pub mod server_download;
pub mod sys_inputs;
pub mod universe;
