use crate::render::{
    PackageDependencyObservation, PackageDependencyTarget, RenderError,
    observe_package_dependencies,
};
use crate::{
    DocumentWorkspace, PackageResolveError, PackageSource, PackageSpec, RenderEnvironment,
};

/// Options controlling one [`prepare_packages`] run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparePackagesOptions {
    max_iterations: usize,
}

impl PreparePackagesOptions {
    /// Create options with the default iteration cap of 10 preflight rounds.
    pub fn new() -> Self {
        Self { max_iterations: 10 }
    }

    /// Cap the number of preflight compile rounds.
    ///
    /// Each round can discover packages imported by packages resolved in the previous round,
    /// so the cap bounds transitive package import depth.
    pub fn max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations.max(1);
        self
    }
}

impl Default for PreparePackagesOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A progress event emitted during [`prepare_packages_with_progress`].
#[derive(Clone, Copy, Debug)]
pub enum PrepareEvent<'a> {
    /// A preflight compile round started.
    PreflightStarted { iteration: usize },

    /// A preflight compile round finished, observing the given missing packages.
    PreflightFinished {
        iteration: usize,
        missing: &'a [PackageSpec],
    },

    /// Resolution of one missing package started.
    ResolveStarted { spec: &'a PackageSpec },

    /// Resolution of one missing package finished.
    ResolveFinished {
        spec: &'a PackageSpec,
        error: Option<&'a PackageResolveError>,
    },
}

/// The result of a [`prepare_packages`] run.
#[derive(Clone, Debug, PartialEq)]
pub struct PackagePreparation {
    environment: RenderEnvironment,
    observation: PackageDependencyObservation,
    resolved: Vec<PackageSpec>,
    unresolved: Vec<(PackageSpec, PackageResolveError)>,
    iterations: usize,
    fixed_point: bool,
}

impl PackagePreparation {
    /// Return the Render Environment enriched with every resolved Package Bundle.
    pub fn environment(&self) -> &RenderEnvironment {
        &self.environment
    }

    /// Consume this preparation and return the enriched Render Environment.
    pub fn into_environment(self) -> RenderEnvironment {
        self.environment
    }

    /// Return whether the run converged: the final preflight compile observed no missing
    /// packages beyond those already recorded as [`unresolved`](Self::unresolved).
    ///
    /// A converged run can still carry unresolved specs — they are reported there rather
    /// than blocking convergence. When this is `false`, the iteration cap or a round
    /// without progress stopped the run before it converged.
    pub fn fixed_point(&self) -> bool {
        self.fixed_point
    }

    /// Return how many preflight compile rounds ran.
    pub fn iterations(&self) -> usize {
        self.iterations
    }

    /// Return the specs resolved into the Render Environment during this run.
    pub fn resolved(&self) -> &[PackageSpec] {
        &self.resolved
    }

    /// Return the specs that failed to resolve, with their failures.
    pub fn unresolved(&self) -> &[(PackageSpec, PackageResolveError)] {
        &self.unresolved
    }

    /// Return the final preflight observation, including its diagnostics.
    pub fn observation(&self) -> &PackageDependencyObservation {
        &self.observation
    }
}

/// Resolve a Typst Project's Observed Package Dependencies during World Preparation.
///
/// This runs preflight compiles and resolves the packages Typst requested through the given
/// Package Source, repeating until a preflight observes no missing packages (packages can
/// import further packages), no resolution makes progress, or the iteration cap is reached.
///
/// Failed specs are recorded instead of aborting: a subsequent real render surfaces Typst's
/// own package errors with source spans. Only an invalid Typst Project or a preflight
/// target whose backend is not part of this build (see [`RenderError::UnsupportedTarget`])
/// is a hard error.
///
/// The preflight compiles are synchronous CPU work inside this async function; on threaded
/// async runtimes, consider running the call inside a blocking task.
pub async fn prepare_packages(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
    target: PackageDependencyTarget,
    source: &(impl PackageSource + ?Sized),
    options: PreparePackagesOptions,
) -> Result<PackagePreparation, RenderError> {
    prepare_packages_with_progress(workspace, environment, target, source, options, |_| {}).await
}

/// [`prepare_packages`] with a progress callback receiving [`PrepareEvent`]s.
pub async fn prepare_packages_with_progress(
    workspace: &DocumentWorkspace,
    environment: &RenderEnvironment,
    target: PackageDependencyTarget,
    source: &(impl PackageSource + ?Sized),
    options: PreparePackagesOptions,
    mut progress: impl FnMut(PrepareEvent<'_>),
) -> Result<PackagePreparation, RenderError> {
    let mut environment = environment.clone();
    let mut resolved: Vec<PackageSpec> = Vec::new();
    let mut unresolved: Vec<(PackageSpec, PackageResolveError)> = Vec::new();
    let mut iterations = 0;
    let mut fixed_point = false;

    let observation = loop {
        iterations += 1;
        progress(PrepareEvent::PreflightStarted {
            iteration: iterations,
        });

        let observation = observe_package_dependencies(workspace, &environment, target)?;
        let missing = observation
            .packages()
            .iter()
            .filter(|spec| environment.package_bundle(spec).is_none())
            .filter(|spec| !unresolved.iter().any(|(failed, _)| failed == *spec))
            .cloned()
            .collect::<Vec<_>>();
        progress(PrepareEvent::PreflightFinished {
            iteration: iterations,
            missing: &missing,
        });

        if missing.is_empty() {
            fixed_point = true;
            break observation;
        }

        let mut progressed = false;
        let mut builder = environment.to_builder();
        for spec in &missing {
            progress(PrepareEvent::ResolveStarted { spec });
            match source.resolve(spec).await {
                Ok(bundle) => {
                    progress(PrepareEvent::ResolveFinished { spec, error: None });
                    builder = builder.replace_package_bundle(bundle);
                    resolved.push(spec.clone());
                    progressed = true;
                }
                Err(error) => {
                    progress(PrepareEvent::ResolveFinished {
                        spec,
                        error: Some(&error),
                    });
                    unresolved.push((spec.clone(), error));
                }
            }
        }
        environment = builder
            .build()
            .expect("replacing package bundles cannot introduce duplicate specs");

        if !progressed || iterations >= options.max_iterations {
            break observation;
        }
    };

    Ok(PackagePreparation {
        environment,
        observation,
        resolved,
        unresolved,
        iterations,
        fixed_point,
    })
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[cfg(feature = "html")]
    use crate::render_html;
    #[allow(unused_imports)]
    use crate::{
        DocumentWorkspace, GatedPackages, MemoryPackages, PackageBundle, PackagePolicy,
        RenderEnvironment,
    };
    #[allow(unused_imports)]
    use std::str::FromStr;

    #[cfg(feature = "html")]
    fn test_package_bundle(name: &str, version: &str, lib: &str) -> PackageBundle {
        let spec = PackageSpec::from_str(&format!("@preview/{name}:{version}"))
            .expect("test spec should parse");

        PackageBundle::builder(spec)
            .file(
                "typst.toml",
                format!(
                    "[package]\nname = \"{name}\"\nversion = \"{version}\"\nentrypoint = \"lib.typ\"\n"
                ),
            )
            .file("lib.typ", lib)
            .build()
            .expect("test package bundle should be valid")
    }

    #[cfg(feature = "html")]
    fn empty_environment() -> RenderEnvironment {
        RenderEnvironment::builder()
            .build()
            .expect("empty environment should build")
    }

    #[cfg(feature = "html")]
    #[test]
    fn prepare_packages_reaches_fixed_point_through_transitive_package_imports() {
        let outer = test_package_bundle(
            "outer",
            "1.0.0",
            "#import \"@preview/inner:1.0.0\": inner\n#let outer = inner",
        );
        let inner = test_package_bundle("inner", "1.0.0", "#let inner = [Nested import.]");
        let source = MemoryPackages::new([outer, inner]).expect("test source should be valid");
        let workspace =
            DocumentWorkspace::from_source("#import \"@preview/outer:1.0.0\": outer\n#outer");

        let preparation = pollster::block_on(prepare_packages(
            &workspace,
            &empty_environment(),
            PackageDependencyTarget::Html,
            &source,
            PreparePackagesOptions::new(),
        ))
        .expect("preparation should run");

        assert!(preparation.fixed_point());
        assert_eq!(preparation.iterations(), 3);
        assert_eq!(preparation.resolved().len(), 2);
        assert!(preparation.unresolved().is_empty());
        assert!(preparation.observation().compile_succeeded());

        let html = render_html(&workspace, preparation.environment())
            .expect("prepared environment should render the transitive package import");
        assert!(html.as_str().contains("Nested import."));
    }

    #[cfg(feature = "html")]
    #[test]
    fn prepare_packages_records_unresolved_packages_without_aborting() {
        let source = MemoryPackages::new([]).expect("empty source should be valid");
        let workspace =
            DocumentWorkspace::from_source("#import \"@preview/missing:1.0.0\": nothing\n#nothing");

        let preparation = pollster::block_on(prepare_packages(
            &workspace,
            &empty_environment(),
            PackageDependencyTarget::Html,
            &source,
            PreparePackagesOptions::new(),
        ))
        .expect("preparation should run");

        assert!(!preparation.fixed_point());
        assert_eq!(preparation.iterations(), 1);
        assert!(preparation.resolved().is_empty());
        assert_eq!(preparation.unresolved().len(), 1);
        assert!(matches!(
            preparation.unresolved()[0],
            (ref spec, PackageResolveError::NotFound { .. })
                if spec.to_string() == "@preview/missing:1.0.0"
        ));
        assert!(!preparation.observation().compile_succeeded());
    }

    #[cfg(feature = "html")]
    #[test]
    fn prepare_packages_reports_progress_events() {
        let example = test_package_bundle("example", "1.0.0", "#let answer = [42]");
        let source = MemoryPackages::new([example]).expect("test source should be valid");
        let workspace =
            DocumentWorkspace::from_source("#import \"@preview/example:1.0.0\": answer\n#answer");

        let mut events = Vec::new();
        pollster::block_on(prepare_packages_with_progress(
            &workspace,
            &empty_environment(),
            PackageDependencyTarget::Html,
            &source,
            PreparePackagesOptions::new(),
            |event| {
                events.push(match event {
                    PrepareEvent::PreflightStarted { iteration } => {
                        format!("preflight-started:{iteration}")
                    }
                    PrepareEvent::PreflightFinished { iteration, missing } => {
                        format!("preflight-finished:{iteration}:{}", missing.len())
                    }
                    PrepareEvent::ResolveStarted { spec } => {
                        format!("resolve-started:{spec}")
                    }
                    PrepareEvent::ResolveFinished { spec, error } => {
                        format!("resolve-finished:{spec}:{ok}", ok = error.is_none())
                    }
                });
            },
        ))
        .expect("preparation should run");

        assert_eq!(
            events,
            vec![
                "preflight-started:1",
                "preflight-finished:1:1",
                "resolve-started:@preview/example:1.0.0",
                "resolve-finished:@preview/example:1.0.0:true",
                "preflight-started:2",
                "preflight-finished:2:0",
            ]
        );
    }

    #[cfg(feature = "html")]
    #[test]
    fn prepare_packages_stops_at_the_iteration_cap_before_converging() {
        // a -> b -> c: each round discovers exactly one more package, so a cap of two
        // rounds resolves a and b but never observes c.
        let a = test_package_bundle("a", "1.0.0", "#import \"@preview/b:1.0.0\": b\n#let a = b");
        let b = test_package_bundle("b", "1.0.0", "#import \"@preview/c:1.0.0\": c\n#let b = c");
        let c = test_package_bundle("c", "1.0.0", "#let c = [Deep import.]");
        let source = MemoryPackages::new([a, b, c]).expect("test source should be valid");
        let workspace = DocumentWorkspace::from_source("#import \"@preview/a:1.0.0\": a\n#a");

        let preparation = pollster::block_on(prepare_packages(
            &workspace,
            &empty_environment(),
            PackageDependencyTarget::Html,
            &source,
            PreparePackagesOptions::new().max_iterations(2),
        ))
        .expect("preparation should run");

        assert!(!preparation.fixed_point());
        assert_eq!(preparation.iterations(), 2);
        assert_eq!(
            preparation
                .resolved()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["@preview/a:1.0.0", "@preview/b:1.0.0"]
        );
        assert!(preparation.unresolved().is_empty());
    }

    #[cfg(feature = "html")]
    #[test]
    fn prepare_packages_stops_when_a_round_makes_no_progress() {
        // Typst aborts compilation at the first failed import, so the second import is
        // only observed once the first resolves; when that second import then fails to
        // resolve, the round makes no progress and the loop terminates without spinning.
        let example = test_package_bundle("example", "1.0.0", "#let answer = [42]");
        let source = MemoryPackages::new([example]).expect("test source should be valid");
        let workspace = DocumentWorkspace::from_source(
            "#import \"@preview/example:1.0.0\": answer\n#import \"@preview/missing:1.0.0\": nothing\n#answer",
        );

        let preparation = pollster::block_on(prepare_packages(
            &workspace,
            &empty_environment(),
            PackageDependencyTarget::Html,
            &source,
            PreparePackagesOptions::new(),
        ))
        .expect("preparation should run");

        assert!(!preparation.fixed_point());
        assert_eq!(preparation.iterations(), 2);
        assert_eq!(preparation.resolved().len(), 1);
        assert_eq!(preparation.unresolved().len(), 1);
        assert!(matches!(
            preparation.unresolved()[0],
            (ref spec, PackageResolveError::NotFound { .. })
                if spec.to_string() == "@preview/missing:1.0.0"
        ));
    }

    #[cfg(feature = "html")]
    #[test]
    fn prepare_packages_records_policy_denials_as_unresolved() {
        let example = test_package_bundle("example", "1.0.0", "#let answer = [42]");
        let source = MemoryPackages::new([example]).expect("test source should be valid");
        let source = GatedPackages::new(source, PackagePolicy::deny_all());
        let workspace =
            DocumentWorkspace::from_source("#import \"@preview/example:1.0.0\": answer\n#answer");

        let preparation = pollster::block_on(prepare_packages(
            &workspace,
            &empty_environment(),
            PackageDependencyTarget::Html,
            &source,
            PreparePackagesOptions::new(),
        ))
        .expect("preparation should run");

        assert!(preparation.resolved().is_empty());
        assert_eq!(preparation.unresolved().len(), 1);
        assert!(matches!(
            preparation.unresolved()[0],
            (ref spec, PackageResolveError::Denied { .. })
                if spec.to_string() == "@preview/example:1.0.0"
        ));
    }

    #[cfg(any(feature = "pdf", feature = "page-images"))]
    #[test]
    fn prepare_packages_observes_dependencies_for_the_paged_target() {
        let example = test_package_bundle("example", "1.0.0", "#let answer = [42]");
        let source = MemoryPackages::new([example]).expect("test source should be valid");
        let workspace =
            DocumentWorkspace::from_source("#import \"@preview/example:1.0.0\": answer\n#answer");

        let preparation = pollster::block_on(prepare_packages(
            &workspace,
            &empty_environment(),
            PackageDependencyTarget::Paged,
            &source,
            PreparePackagesOptions::new(),
        ))
        .expect("preparation should run");

        assert!(preparation.fixed_point());
        assert_eq!(preparation.resolved().len(), 1);
        assert!(preparation.observation().compile_succeeded());
    }
}
