//! Route components. Each page frames one or more `examples` components with
//! prose, a docs link, and the example's own source (via `code!`).

mod basics;
mod downloads;
mod packages;
mod projects;
mod server;

pub use basics::{Editor, Home, Minimal, SysInputs};
pub use downloads::{PageImagesDownload, PdfDownload};
pub use packages::{EmbeddedPackage, PackagePolicyPage, UniversePackages};
pub use projects::{ImportProject, MultiFile, ProjectPacks};
pub use server::ServerRendering;
