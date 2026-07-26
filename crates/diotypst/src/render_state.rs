use crate::{RenderArtifact, RenderError, RenderFormat, render_artifact_world};
use typst::World;

/// Owned, cheaply cloneable handle to a Complete Typst World.
#[derive(Clone)]
pub struct SharedWorld {
    world: std::sync::Arc<dyn World>,
}

impl SharedWorld {
    /// Share an owned Complete Typst World with rendering hooks and components.
    pub fn new(world: impl World + 'static) -> Self {
        Self {
            world: std::sync::Arc::new(world),
        }
    }

    /// Borrow the Complete Typst World.
    pub fn as_world(&self) -> &dyn World {
        self.world.as_ref()
    }
}

impl std::fmt::Debug for SharedWorld {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SharedWorld")
            .finish_non_exhaustive()
    }
}

impl PartialEq for SharedWorld {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.world, &other.world)
    }
}

impl Eq for SharedWorld {}

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

/// Reactively render an owned Complete Typst World.
///
/// Replace the [`SharedWorld`] handle when any world input changes. This hook owns
/// Render State only; World Preparation and World construction remain separate.
#[cfg(feature = "dioxus")]
pub fn use_world_render(
    world: SharedWorld,
    format: RenderFormat,
) -> dioxus::prelude::Signal<HeadlessRender> {
    use dioxus::prelude::{ReadableExt, WritableExt, use_effect, use_reactive, use_signal};

    let mut renderer = {
        let world = world.clone();
        use_signal(move || {
            let mut render = HeadlessRender::new();
            render.render_world(world.as_world(), format);
            render
        })
    };
    let mut last_rendered = {
        let world = world.clone();
        use_signal(move || (world, format))
    };
    use_effect(use_reactive((&world, &format), move |(world, format)| {
        if *last_rendered.peek() == (world.clone(), format) {
            return;
        }
        last_rendered.set((world.clone(), format));
        renderer.write().render_world(world.as_world(), format);
    }));

    renderer
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
