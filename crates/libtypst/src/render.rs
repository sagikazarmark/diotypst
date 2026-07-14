#[cfg(feature = "html")]
use crate::artifact::HtmlArtifact;
#[cfg(feature = "page-images")]
use crate::artifact::PageImageOptions;
#[cfg(feature = "pdf")]
use crate::artifact::PdfArtifact;
#[cfg(feature = "page-images")]
use crate::artifact::{PageImage, PageImagesArtifact};
use crate::artifact::{RenderArtifact, RenderFormat};
use crate::diagnostics::RenderDiagnostic;
#[cfg(any(feature = "pdf", feature = "page-images", feature = "html"))]
use crate::diagnostics::to_render_diagnostics;
use crate::observe::RecordingWorld;
use crate::{
    DocumentWorkspace, PackageSpec, RenderEnvironment, SandboxedWorld, WorkspaceValidationError,
};
use typst::World;
#[cfg(any(feature = "pdf", feature = "page-images", feature = "html"))]
use typst::diag::SourceDiagnostic;
#[cfg(any(feature = "pdf", feature = "page-images", feature = "html"))]
use typst::foundations::Output;
#[cfg(feature = "pdf")]
use typst::foundations::Smart;
#[cfg(feature = "page-images")]
use typst::utils::Scalar;
#[cfg(feature = "html")]
use typst_html::{HtmlDocument, HtmlOptions};
#[cfg(any(feature = "pdf", feature = "page-images"))]
use typst_layout::PagedDocument;

/// Render a Typst Project to a PDF artifact.
#[cfg(feature = "pdf")]
pub fn render_pdf(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
) -> Result<PdfArtifact, RenderError> {
    let world = SandboxedWorld::new(workspace.clone(), environment.clone())
        .map_err(RenderError::Workspace)?;
    render_pdf_world(&world)
}

/// Render a Complete Typst World to a PDF artifact.
#[cfg(feature = "pdf")]
pub fn render_pdf_world(world: &dyn World) -> Result<PdfArtifact, RenderError> {
    let document: PagedDocument = compile_document(world)?;
    let bytes = typst_pdf::pdf(
        &document,
        &typst_pdf::PdfOptions {
            ident: Smart::Auto,
            ..typst_pdf::PdfOptions::default()
        },
    )
    .map_err(|diagnostics| RenderError::Diagnostics(to_render_diagnostics(world, diagnostics)))?;

    Ok(PdfArtifact { bytes })
}

/// Render a Typst Project to one PNG Page Image per rendered Typst page.
#[cfg(feature = "page-images")]
pub fn render_page_images(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
    options: PageImageOptions,
) -> Result<PageImagesArtifact, RenderError> {
    let world = SandboxedWorld::new(workspace.clone(), environment.clone())
        .map_err(RenderError::Workspace)?;
    render_page_images_world(&world, options)
}

/// Render a Complete Typst World to one PNG Page Image per rendered Typst page.
#[cfg(feature = "page-images")]
pub fn render_page_images_world(
    world: &dyn World,
    options: PageImageOptions,
) -> Result<PageImagesArtifact, RenderError> {
    let document: PagedDocument = compile_document(world)?;
    let render_options = typst_render::RenderOptions {
        pixel_per_pt: Scalar::new(options.pixel_per_pt() as f64),
        ..typst_render::RenderOptions::default()
    };
    let pages = document
        .pages()
        .iter()
        .map(|page| {
            let pixmap = typst_render::render(page, &render_options);
            let width = pixmap.width();
            let height = pixmap.height();
            let bytes = pixmap
                .encode_png()
                .map_err(|error| RenderError::ImageEncoding(error.to_string()))?;

            Ok(PageImage {
                bytes,
                width,
                height,
            })
        })
        .collect::<Result<Vec<_>, RenderError>>()?;

    Ok(PageImagesArtifact { pages })
}

/// Render a Typst Project to a self-contained semantic HTML artifact.
#[cfg(feature = "html")]
pub fn render_html(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
) -> Result<HtmlArtifact, RenderError> {
    let world = SandboxedWorld::for_html(workspace.clone(), environment.clone())
        .map_err(RenderError::Workspace)?;
    render_html_world(&world)
}

/// Render a Complete Typst World to a self-contained semantic HTML artifact.
#[cfg(feature = "html")]
pub fn render_html_world(world: &dyn World) -> Result<HtmlArtifact, RenderError> {
    let document: HtmlDocument = compile_document(world)?;
    let html =
        typst_html::html(&document, &HtmlOptions { pretty: true }).map_err(|diagnostics| {
            RenderError::Diagnostics(to_render_diagnostics(world, diagnostics))
        })?;

    Ok(HtmlArtifact { html })
}

/// Render a Typst Project to the selected Render Artifact format.
///
/// Formats whose backend is not part of this build (see the `pdf`, `page-images`, and
/// `html` features) report [`RenderError::UnsupportedFormat`] — a Render Capability is
/// never silently substituted.
#[cfg_attr(
    not(any(feature = "pdf", feature = "page-images", feature = "html")),
    allow(unused_variables)
)]
pub fn render_artifact(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
    format: RenderFormat,
) -> Result<RenderArtifact, RenderError> {
    match format {
        #[cfg(feature = "pdf")]
        RenderFormat::Pdf => render_pdf(workspace, environment).map(RenderArtifact::Pdf),
        #[cfg(feature = "page-images")]
        RenderFormat::PageImages(options) => {
            render_page_images(workspace, environment, options).map(RenderArtifact::PageImages)
        }
        #[cfg(feature = "html")]
        RenderFormat::Html => render_html(workspace, environment).map(RenderArtifact::Html),
        #[allow(unreachable_patterns)]
        format => Err(RenderError::UnsupportedFormat { format }),
    }
}

/// Render a Complete Typst World to the selected Render Artifact format.
///
/// Formats whose backend is not part of this build (see the `pdf`, `page-images`, and
/// `html` features) report [`RenderError::UnsupportedFormat`].
#[cfg_attr(
    not(any(feature = "pdf", feature = "page-images", feature = "html")),
    allow(unused_variables)
)]
pub fn render_artifact_world(
    world: &dyn World,
    format: RenderFormat,
) -> Result<RenderArtifact, RenderError> {
    match format {
        #[cfg(feature = "pdf")]
        RenderFormat::Pdf => render_pdf_world(world).map(RenderArtifact::Pdf),
        #[cfg(feature = "page-images")]
        RenderFormat::PageImages(options) => {
            render_page_images_world(world, options).map(RenderArtifact::PageImages)
        }
        #[cfg(feature = "html")]
        RenderFormat::Html => render_html_world(world).map(RenderArtifact::Html),
        #[allow(unreachable_patterns)]
        format => Err(RenderError::UnsupportedFormat { format }),
    }
}

/// Compile target used while observing package dependencies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageDependencyTarget {
    /// Compile as a paged document, matching PDF and Page Image rendering.
    Paged,

    /// Compile as an HTML document, matching semantic HTML rendering.
    Html,
}

/// Package dependencies observed during a preflight compile.
///
/// These packages are evidence from one compile pass. They are not a canonical template contract:
/// dynamic Typst code, target-specific branches, inputs, missing resources, and diagnostics can all affect
/// what is observed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageDependencyObservation {
    packages: Vec<PackageSpec>,
    diagnostics: Vec<RenderDiagnostic>,
    compile_succeeded: bool,
}

impl PackageDependencyObservation {
    /// Return exact Package Specs requested by Typst during the preflight compile.
    pub fn packages(&self) -> &[PackageSpec] {
        &self.packages
    }

    /// Return diagnostics produced by the preflight compile.
    pub fn diagnostics(&self) -> &[RenderDiagnostic] {
        &self.diagnostics
    }

    /// Return whether the preflight compile completed successfully.
    pub fn compile_succeeded(&self) -> bool {
        self.compile_succeeded
    }
}

/// Run a preflight compile and return Package Specs observed during that compile.
///
/// Observed packages are useful for warming package caches and validating template metadata. They should not
/// replace an explicit package list on a template because Typst imports can be dynamic or target-specific.
///
/// The preflight compiles for the given target, so its backend must be part of this
/// build: the Paged target needs the `pdf` or `page-images` feature, the Html target
/// needs `html`. Absent backends report [`RenderError::UnsupportedTarget`].
pub fn observe_package_dependencies(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
    target: PackageDependencyTarget,
) -> Result<PackageDependencyObservation, RenderError> {
    let world = match target {
        PackageDependencyTarget::Paged => {
            SandboxedWorld::new(workspace.clone(), environment.clone())
                .map_err(RenderError::Workspace)?
        }
        PackageDependencyTarget::Html => {
            SandboxedWorld::for_html(workspace.clone(), environment.clone())
                .map_err(RenderError::Workspace)?
        }
    };

    observe_package_dependencies_world(&world, target)
}

/// Run a preflight compile against a Complete Typst World and return observed Package Specs.
///
/// For HTML preflight, the supplied world must enable Typst's HTML feature, just like raw HTML rendering.
/// The target's backend must be part of this build; see [`observe_package_dependencies`].
#[cfg_attr(
    not(any(feature = "pdf", feature = "page-images", feature = "html")),
    allow(unused_variables, unreachable_code)
)]
pub fn observe_package_dependencies_world(
    world: &dyn World,
    target: PackageDependencyTarget,
) -> Result<PackageDependencyObservation, RenderError> {
    let world = RecordingWorld::new(world);
    let (compile_succeeded, diagnostics) = match target {
        #[cfg(any(feature = "pdf", feature = "page-images"))]
        PackageDependencyTarget::Paged => compile_for_observation::<PagedDocument>(&world),
        #[cfg(feature = "html")]
        PackageDependencyTarget::Html => compile_for_observation::<HtmlDocument>(&world),
        #[allow(unreachable_patterns)]
        target => return Err(RenderError::UnsupportedTarget { target }),
    };

    Ok(PackageDependencyObservation {
        packages: world.observed_packages(),
        diagnostics,
        compile_succeeded,
    })
}

/// A render failure.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderError {
    /// The Typst Project was not valid enough to render.
    Workspace(WorkspaceValidationError),

    /// Typst reported diagnostics while compiling or exporting.
    Diagnostics(Vec<RenderDiagnostic>),

    /// A rendered Page Image could not be encoded.
    ImageEncoding(String),

    /// The requested Render Format's backend is not part of this build.
    ///
    /// Render Capabilities are features: `pdf`, `page-images`, and `html`.
    UnsupportedFormat {
        /// The requested format.
        format: RenderFormat,
    },

    /// The preflight target's backend is not part of this build.
    ///
    /// The Paged target needs the `pdf` or `page-images` feature; the Html target needs `html`.
    UnsupportedTarget {
        /// The requested preflight target.
        target: PackageDependencyTarget,
    },
}

#[cfg(any(feature = "pdf", feature = "page-images", feature = "html"))]
fn compile_document<D>(world: &dyn World) -> Result<D, RenderError>
where
    D: Output,
{
    let warned = typst::compile::<D>(world);
    let missing_font_warnings = warned
        .warnings
        .into_iter()
        .filter(is_missing_font_warning)
        .collect::<Vec<_>>();

    if !missing_font_warnings.is_empty() {
        return Err(RenderError::Diagnostics(to_render_diagnostics(
            world,
            missing_font_warnings,
        )));
    }

    warned
        .output
        .map_err(|diagnostics| RenderError::Diagnostics(to_render_diagnostics(world, diagnostics)))
}

#[cfg(any(feature = "pdf", feature = "page-images", feature = "html"))]
fn compile_for_observation<D>(world: &dyn World) -> (bool, Vec<RenderDiagnostic>)
where
    D: Output,
{
    let warned = typst::compile::<D>(world);
    let mut diagnostics = warned.warnings.into_iter().collect::<Vec<_>>();
    let compile_succeeded = match warned.output {
        Ok(_) => true,
        Err(errors) => {
            diagnostics.extend(errors);
            false
        }
    };

    (compile_succeeded, to_render_diagnostics(world, diagnostics))
}

#[cfg(any(feature = "pdf", feature = "page-images", feature = "html"))]
fn is_missing_font_warning(diagnostic: &SourceDiagnostic) -> bool {
    diagnostic
        .message
        .to_string()
        .starts_with("unknown font family:")
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use crate::FontSet;

    // Guards the string coupling in is_missing_font_warning: Typst reports missing fonts
    // as a warning whose message this crate matches by prefix. If a Typst upgrade changes
    // the message, this test fails instead of missing fonts silently rendering as tofu.
    #[cfg(feature = "pdf")]
    #[test]
    fn missing_fonts_fail_rendering_via_typsts_warning_message() {
        let workspace =
            DocumentWorkspace::from_source("#set text(font: \"no-such-font-family\")\nHello");
        let environment = RenderEnvironment::builder()
            .font_set(FontSet::empty())
            .build()
            .expect("environment should build");

        let error = render_pdf(&workspace, &environment)
            .expect_err("rendering without the requested font should fail");

        let RenderError::Diagnostics(diagnostics) = error else {
            panic!("missing fonts should surface as diagnostics, got {error:?}");
        };
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().starts_with("unknown font family:")),
            "Typst's missing-font message changed; update is_missing_font_warning: {diagnostics:?}"
        );
    }

    // The Render Capability contract: an absent backend is an explicit error, never a
    // silent fallback.
    #[cfg(not(feature = "pdf"))]
    #[test]
    fn absent_pdf_backend_reports_an_unsupported_format() {
        let workspace = DocumentWorkspace::from_source("Hello");
        let environment = RenderEnvironment::default();

        let error = render_artifact(&workspace, &environment, RenderFormat::Pdf)
            .expect_err("the pdf feature is off, so PDF rendering must be unavailable");

        assert!(matches!(
            error,
            RenderError::UnsupportedFormat {
                format: RenderFormat::Pdf
            }
        ));
    }
}
