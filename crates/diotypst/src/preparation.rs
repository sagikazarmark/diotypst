//! Dioxus-owned World Preparation state for package resolution.

use libtypst::{PackageResolveError, PackageSpec, RenderEnvironment, RenderError};

/// Status of one package inside a World Preparation run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackagePreparationStatus {
    /// The package was observed as missing and waits for resolution.
    Queued,

    /// The package is being resolved through the Package Source.
    Downloading,

    /// The package bundle is part of the prepared Render Environment.
    Ready,

    /// A Package Policy denied the package.
    Denied,

    /// The package failed to resolve.
    Failed,
}

/// One package tracked by a World Preparation run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackagePreparationEntry {
    spec: PackageSpec,
    status: PackagePreparationStatus,
    message: Option<String>,
}

impl PackagePreparationEntry {
    /// Return the exact Package Spec this entry tracks.
    pub fn spec(&self) -> &PackageSpec {
        &self.spec
    }

    /// Return the package's preparation status.
    pub fn status(&self) -> PackagePreparationStatus {
        self.status
    }

    /// Return failure detail for denied or failed packages.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// Phase of a World Preparation run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldPreparationPhase {
    /// No preparation has run yet.
    Idle,

    /// Preflight compiles and package resolution are in progress.
    Preparing,

    /// The prepared Render Environment reflects the latest inputs.
    Ready,

    /// Preparation could not run, such as an invalid Typst Project or a preflight
    /// target whose Render Capability is not part of this build.
    Failed,
}

/// Observable state of a World Preparation run.
///
/// The environment is always usable: it starts as the base Render Environment and gains
/// resolved Package Bundles as preparation completes, so renders during preparation degrade
/// to missing-package diagnostics instead of blocking.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldPreparationState {
    phase: WorldPreparationPhase,
    packages: Vec<PackagePreparationEntry>,
    environment: RenderEnvironment,
    error: Option<RenderError>,
}

impl WorldPreparationState {
    /// Create idle preparation state around a base Render Environment.
    pub fn new(environment: RenderEnvironment) -> Self {
        Self {
            phase: WorldPreparationPhase::Idle,
            packages: Vec::new(),
            environment,
            error: None,
        }
    }

    /// Return the current preparation phase.
    pub fn phase(&self) -> WorldPreparationPhase {
        self.phase
    }

    /// Return the packages tracked by the current run.
    pub fn packages(&self) -> &[PackagePreparationEntry] {
        &self.packages
    }

    /// Return the Render Environment to render with.
    pub fn environment(&self) -> &RenderEnvironment {
        &self.environment
    }

    /// Return the preparation failure when the phase is `Failed`.
    pub fn error(&self) -> Option<&RenderError> {
        self.error.as_ref()
    }

    /// Return whether the prepared environment reflects the latest inputs.
    pub fn is_ready(&self) -> bool {
        self.phase == WorldPreparationPhase::Ready
    }

    /// Start a preparation run, clearing tracked packages while keeping the environment.
    pub fn begin(&mut self) {
        self.phase = WorldPreparationPhase::Preparing;
        self.packages.clear();
        self.error = None;
    }

    /// Track a missing package as queued for resolution.
    pub fn queue(&mut self, spec: &PackageSpec) {
        if !self.packages.iter().any(|entry| entry.spec() == spec) {
            self.packages.push(PackagePreparationEntry {
                spec: spec.clone(),
                status: PackagePreparationStatus::Queued,
                message: None,
            });
        }
    }

    /// Update the status of a tracked package.
    pub fn set_status(
        &mut self,
        spec: &PackageSpec,
        status: PackagePreparationStatus,
        message: Option<String>,
    ) {
        if let Some(entry) = self.packages.iter_mut().find(|entry| entry.spec() == spec) {
            entry.status = status;
            entry.message = message;
        }
    }

    /// Finish the run successfully with the prepared Render Environment.
    pub fn finish(&mut self, environment: RenderEnvironment) {
        self.phase = WorldPreparationPhase::Ready;
        self.environment = environment;
    }

    /// Finish the run with a preparation failure.
    pub fn fail(&mut self, error: RenderError) {
        self.phase = WorldPreparationPhase::Failed;
        self.error = Some(error);
    }

    /// Apply one [`PrepareEvent`](crate::PrepareEvent) from the preparation loop to this state.
    pub fn apply_prepare_event(&mut self, event: libtypst::PrepareEvent<'_>) {
        use libtypst::PrepareEvent;

        match event {
            PrepareEvent::PreflightStarted { .. } => {}
            PrepareEvent::PreflightFinished { missing, .. } => {
                for spec in missing {
                    self.queue(spec);
                }
            }
            PrepareEvent::ResolveStarted { spec } => {
                self.set_status(spec, PackagePreparationStatus::Downloading, None);
            }
            PrepareEvent::ResolveFinished { spec, error } => match error {
                None => self.set_status(spec, PackagePreparationStatus::Ready, None),
                Some(PackageResolveError::Denied { .. }) => self.set_status(
                    spec,
                    PackagePreparationStatus::Denied,
                    Some("denied by package policy".to_owned()),
                ),
                Some(error) => self.set_status(
                    spec,
                    PackagePreparationStatus::Failed,
                    Some(format!("{error:?}")),
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libtypst::PrepareEvent;
    use std::str::FromStr;

    fn spec(text: &str) -> PackageSpec {
        PackageSpec::from_str(text).expect("test spec should parse")
    }

    fn base_environment() -> RenderEnvironment {
        RenderEnvironment::builder()
            .build()
            .expect("empty environment should build")
    }

    #[test]
    fn preparation_state_tracks_package_lifecycle_from_events() {
        let mut state = WorldPreparationState::new(base_environment());
        assert_eq!(state.phase(), WorldPreparationPhase::Idle);

        state.begin();
        assert_eq!(state.phase(), WorldPreparationPhase::Preparing);

        let cetz = spec("@preview/cetz:0.4.2");
        let denied = spec("@preview/tablex:0.0.9");
        state.apply_prepare_event(PrepareEvent::PreflightFinished {
            iteration: 1,
            missing: &[cetz.clone(), denied.clone()],
        });
        assert_eq!(state.packages().len(), 2);
        assert_eq!(
            state.packages()[0].status(),
            PackagePreparationStatus::Queued
        );

        state.apply_prepare_event(PrepareEvent::ResolveStarted { spec: &cetz });
        assert_eq!(
            state.packages()[0].status(),
            PackagePreparationStatus::Downloading
        );

        state.apply_prepare_event(PrepareEvent::ResolveFinished {
            spec: &cetz,
            error: None,
        });
        assert_eq!(
            state.packages()[0].status(),
            PackagePreparationStatus::Ready
        );

        state.apply_prepare_event(PrepareEvent::ResolveFinished {
            spec: &denied,
            error: Some(&PackageResolveError::Denied {
                spec: denied.clone(),
            }),
        });
        assert_eq!(
            state.packages()[1].status(),
            PackagePreparationStatus::Denied
        );
        assert_eq!(
            state.packages()[1].message(),
            Some("denied by package policy")
        );

        state.finish(base_environment());
        assert!(state.is_ready());
    }

    #[test]
    fn preparation_state_keeps_prior_environment_while_preparing() {
        let enriched = RenderEnvironment::builder()
            .input("name", "prior")
            .build()
            .expect("environment should build");
        let mut state = WorldPreparationState::new(base_environment());
        state.begin();
        state.finish(enriched.clone());

        state.begin();
        assert_eq!(state.phase(), WorldPreparationPhase::Preparing);
        assert_eq!(state.environment(), &enriched);
        assert!(state.packages().is_empty());
    }
}
