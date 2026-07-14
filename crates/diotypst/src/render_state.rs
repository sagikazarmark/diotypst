use crate::{
    DocumentWorkspace, RenderArtifact, RenderEnvironment, RenderError, RenderFormat,
    render_artifact, render_artifact_world,
};
use typst::World;

/// Reusable headless render flow for Dioxus-owned UI state.
#[derive(Clone, Debug, PartialEq)]
pub struct HeadlessRender {
    state: RenderState<RenderArtifact>,
    format: Option<RenderFormat>,
}

impl HeadlessRender {
    /// Create an empty headless render flow.
    pub fn new() -> Self {
        Self {
            state: RenderState::new(),
            format: None,
        }
    }

    /// Trigger rendering for the selected Render Format.
    pub fn render(
        &mut self,
        workspace: &DocumentWorkspace,
        environment: &RenderEnvironment,
        format: RenderFormat,
    ) {
        if self.format != Some(format) {
            self.state = RenderState::new();
            self.format = Some(format);
        }

        self.state
            .update(render_artifact(workspace, environment, format));
    }

    /// Trigger rendering for the selected Render Format from a Complete Typst World.
    pub fn render_world(&mut self, world: &dyn World, format: RenderFormat) {
        if self.format != Some(format) {
            self.state = RenderState::new();
            self.format = Some(format);
        }

        self.state.update(render_artifact_world(world, format));
    }

    /// Return the current headless Render State.
    pub fn state(&self) -> &RenderState<RenderArtifact> {
        &self.state
    }
}

impl Default for HeadlessRender {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a Dioxus signal for a reusable headless Typst render flow.
///
/// This is the lower seam for custom Complete Typst Worlds (see
/// [`HeadlessRender::render_world`]); document flows should use
/// [`use_render_session`](crate::use_render_session), which owns World Preparation and
/// Render Policy scheduling as well.
#[cfg(feature = "dioxus")]
pub fn use_typst_render() -> dioxus::prelude::Signal<HeadlessRender> {
    dioxus::prelude::use_signal(HeadlessRender::new)
}

/// Headless state for a render flow that may retain a Stale Artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderState<T> {
    status: RenderStatus,
    artifact: Option<T>,
    error: Option<RenderError>,
}

impl<T> RenderState<T> {
    /// Create an empty render state.
    pub fn new() -> Self {
        Self {
            status: RenderStatus::Empty,
            artifact: None,
            error: None,
        }
    }

    /// Apply a render result to this state.
    pub fn update(&mut self, result: Result<T, RenderError>) {
        match result {
            Ok(artifact) => {
                self.status = RenderStatus::Current;
                self.artifact = Some(artifact);
                self.error = None;
            }
            Err(error) => {
                self.status = if self.artifact.is_some() {
                    RenderStatus::Stale
                } else {
                    RenderStatus::Failed
                };
                self.error = Some(error);
            }
        }
    }

    /// Return whether the state is empty, current, stale, or failed.
    pub fn status(&self) -> RenderStatus {
        self.status
    }

    /// Return the current or stale Render Artifact, if available.
    pub fn artifact(&self) -> Option<&T> {
        self.artifact.as_ref()
    }

    /// Return the most recent render error, if available.
    pub fn error(&self) -> Option<&RenderError> {
        self.error.as_ref()
    }
}

impl<T> Default for RenderState<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of a headless render flow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderStatus {
    /// No render has completed or failed yet.
    Empty,

    /// The artifact matches the latest render input.
    Current,

    /// The latest render failed, but a previous successful artifact remains available.
    Stale,

    /// The latest render failed and no artifact is available.
    Failed,
}
