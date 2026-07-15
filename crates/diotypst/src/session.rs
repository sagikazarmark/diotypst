#[cfg(feature = "dioxus")]
use crate::RenderFormat;
use crate::provider::SharedPackageSource;
use crate::{DocumentWorkspace, PageImageOptions, RenderEnvironment};
use typst::foundations::Dict;

/// Document input accepted by a Render Session and the Typst Component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypstInput {
    /// One Typst source string rendered as `main.typ`.
    Source(String),

    /// A complete Typst Project.
    Workspace(DocumentWorkspace),
}

impl TypstInput {
    /// Create document input from one source string.
    pub fn source(source: impl Into<String>) -> Self {
        Self::Source(source.into())
    }

    #[cfg(feature = "dioxus")]
    pub(crate) fn to_workspace(&self) -> DocumentWorkspace {
        match self {
            Self::Source(source) => DocumentWorkspace::from_source(source.clone()),
            Self::Workspace(workspace) => workspace.clone(),
        }
    }
}

/// Explicit view format rendered by a Render Session and the Typst Component.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TypstView {
    /// Render semantic HTML.
    Html,

    /// Render a PDF inside an iframe.
    PdfFrame,

    /// Render one image per Typst page.
    PageImages(PageImageOptions),
}

impl TypstView {
    #[cfg(feature = "dioxus")]
    pub(crate) fn render_format(self) -> RenderFormat {
        match self {
            Self::Html => RenderFormat::Html,
            Self::PdfFrame => RenderFormat::Pdf,
            Self::PageImages(options) => RenderFormat::PageImages(options),
        }
    }
}

/// Per-call overrides for a Render Session.
///
/// Every unset field falls back to the Typst Provider defaults, then to built-in
/// defaults (an empty Render Environment, no Package Source). The fallback order is
/// call-site → provider → built-in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderSessionOptions {
    environment: Option<RenderEnvironment>,
    package_source: Option<SharedPackageSource>,
    inputs: Option<Dict>,
}

impl RenderSessionOptions {
    /// Create options that use provider defaults for everything.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use an explicit Render Environment instead of the provider default.
    pub fn environment(mut self, environment: RenderEnvironment) -> Self {
        self.environment = Some(environment);
        self
    }

    /// Use an explicit Package Source for World Preparation instead of the provider default.
    pub fn package_source(mut self, package_source: SharedPackageSource) -> Self {
        self.package_source = Some(package_source);
        self
    }

    /// Merge Typst `sys.inputs` values over the resolved Render Environment.
    pub fn merge_inputs(mut self, inputs: Dict) -> Self {
        let mut merged = self.inputs.take().unwrap_or_default();
        merged += inputs;
        self.inputs = Some(merged);
        self
    }

    /// Add or replace one Typst value visible through `sys.inputs`.
    pub fn input(self, key: impl Into<String>, value: impl typst::foundations::IntoValue) -> Self {
        let mut inputs = Dict::new();
        inputs.insert(
            key.into().into(),
            typst::foundations::IntoValue::into_value(value),
        );

        self.merge_inputs(inputs)
    }
}

#[cfg(feature = "dioxus")]
pub use hook::{RenderSession, use_render_session};

#[cfg(feature = "dioxus")]
mod hook {
    use super::{RenderSessionOptions, TypstInput, TypstView};
    use crate::preparation::WorldPreparationState;
    use crate::provider::use_typst_defaults;
    use crate::render_state::HeadlessRender;
    use crate::{
        PackageDependencyTarget, PreparePackagesOptions, RenderEnvironment,
        prepare_packages_with_progress,
    };
    use dioxus::hooks::{Resource, use_resource};
    use dioxus::prelude::{ReadableExt, Signal, WritableExt, use_effect, use_reactive, use_signal};

    /// Handle to a Render Session: the declarative flow that keeps one Typst Project
    /// rendered.
    ///
    /// Rendering uses an explicit Render Environment, explicit Font Set, and fixed Render
    /// Date, so it is deterministic; a session is reactive memoization of a pure function:
    /// it renders whenever the Typst Project, the view, or the prepared Render
    /// Environment changes, and never needs an imperative trigger. The Render Policy is
    /// the caller's signal wiring: pass a live signal for always-on rendering, a
    /// debounced signal for live preview, or a signal committed on user action for
    /// explicit rendering. Rendering is synchronous CPU work, so avoid feeding raw
    /// keystrokes for non-trivial documents.
    ///
    /// The handle exposes two dimensions: [`state`](Self::state) is the render dimension
    /// (Render State with the current or Stale Artifact and Diagnostics), and
    /// [`preparation`](Self::preparation) is the World Preparation dimension (phase and
    /// per-package progress). Renders proceed during preparation with the environment
    /// prepared so far, degrading to missing-package diagnostics instead of blocking;
    /// the enriched environment re-renders the session when preparation completes.
    #[derive(Clone, Copy)]
    pub struct RenderSession {
        renderer: Signal<HeadlessRender>,
        preparation: Signal<WorldPreparationState>,
        preparation_resource: Resource<()>,
    }

    impl RenderSession {
        /// Return the render dimension: the headless Render State signal.
        pub fn state(&self) -> Signal<HeadlessRender> {
            self.renderer
        }

        /// Return the World Preparation dimension: phase and per-package progress.
        pub fn preparation(&self) -> Signal<WorldPreparationState> {
            self.preparation
        }

        /// Return the Render Environment the session renders with right now.
        pub fn environment(&self) -> RenderEnvironment {
            self.preparation.read().environment().clone()
        }

        /// Re-run World Preparation, retrying failed package resolutions.
        pub fn restart_preparation(&mut self) {
            self.preparation_resource.restart();
        }
    }

    /// Create a Render Session for one document input and view format.
    ///
    /// The first render happens synchronously during the first hook run, so first paint
    /// and server-side rendering emit the document. After that the session renders
    /// whenever its inputs change: a changed Typst Project or view renders immediately
    /// with the environment prepared so far, and a World Preparation run that enriches
    /// the environment (resolved Package Bundles) re-renders with it. Dependencies
    /// resolve call-site options first, then Typst Provider defaults, then built-in
    /// defaults.
    pub fn use_render_session(
        input: TypstInput,
        view: TypstView,
        options: RenderSessionOptions,
    ) -> RenderSession {
        let defaults = use_typst_defaults();
        let environment = {
            let base = options
                .environment
                .clone()
                .unwrap_or_else(|| defaults.environment().clone());
            match &options.inputs {
                Some(inputs) => base
                    .to_builder()
                    .merge_inputs(inputs.clone())
                    .build()
                    .expect("merging sys.inputs cannot invalidate a valid render environment"),
                None => base,
            }
        };
        let source = options
            .package_source
            .clone()
            .or_else(|| defaults.package_source().cloned());
        let workspace = input.to_workspace();
        let target = match view {
            TypstView::Html => PackageDependencyTarget::Html,
            TypstView::PdfFrame | TypstView::PageImages(_) => PackageDependencyTarget::Paged,
        };

        // --- World Preparation dimension ---
        let mut preparation_inputs =
            use_signal(|| (workspace.clone(), environment.clone(), target));
        use_effect(use_reactive(
            (&workspace, &environment, &target),
            move |value| {
                if *preparation_inputs.peek() != value {
                    preparation_inputs.set(value);
                }
            },
        ));

        let mut preparation = use_signal(|| WorldPreparationState::new(environment.clone()));
        let preparation_resource = use_resource(move || {
            let (workspace, base_environment, target) = preparation_inputs.read().clone();
            let source = source.clone();

            async move {
                let Some(source) = source else {
                    let mut state = preparation.peek().clone();
                    state.begin();
                    state.finish(base_environment);
                    preparation.set(state);
                    return;
                };

                {
                    let mut preparation = preparation.write();
                    preparation.begin();
                }

                let result = prepare_packages_with_progress(
                    &workspace,
                    &base_environment,
                    target,
                    &source,
                    PreparePackagesOptions::new(),
                    |event| preparation.write().apply_prepare_event(event),
                )
                .await;

                match result {
                    Ok(prepared) => preparation.write().finish(prepared.into_environment()),
                    Err(error) => preparation.write().fail(error),
                }
            }
        });

        // --- Render dimension ---
        // The first render happens synchronously in the signal initializer so first
        // paint and server-side rendering emit the document.
        let renderer = {
            let workspace = workspace.clone();
            let environment = environment.clone();
            use_signal(move || {
                let mut headless = HeadlessRender::new();
                headless.render(&workspace, &environment, view.render_format());
                headless
            })
        };

        // One declarative rule: render whenever the triple (Typst Project, view,
        // prepared Render Environment) differs from what was rendered last. This covers
        // input changes (rendered immediately with the environment prepared so far,
        // degrading to missing-package diagnostics) and preparation enrichment (the
        // resolved Package Bundles land and re-render); a preparation run that merely
        // echoes an already-rendered environment changes nothing and renders nothing.
        let mut last_rendered = {
            let workspace = workspace.clone();
            let environment = environment.clone();
            use_signal(move || (workspace, view, environment))
        };
        use_effect(use_reactive(
            (&workspace, &view),
            move |(workspace, view)| {
                let environment = preparation.read().environment().clone();
                let triple = (workspace, view, environment);
                if *last_rendered.peek() == triple {
                    return;
                }

                last_rendered.set(triple.clone());
                let mut renderer = renderer;
                renderer
                    .write()
                    .render(&triple.0, &triple.2, triple.1.render_format());
            },
        ));

        RenderSession {
            renderer,
            preparation,
            preparation_resource,
        }
    }
}
