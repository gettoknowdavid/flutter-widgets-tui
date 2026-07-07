/// What `update()` hands back to the event loop after processing one
/// `Message`.
///
/// We use a named-field struct (not a bare `bool` or a tuple) so that
/// adding a new field later — e.g. `toast: Option<String>` for a "copied to
/// clipboard" confirmation — doesn't break every call site's destructuring
/// pattern.
#[derive(Debug, Default)]
pub struct UpdateOutcome {
    /// Side effects to dispatch. Usually empty.
    pub commands: Vec<crate::command::Command>,

    /// Whether `AppState` actually changed in a way that requires a new
    /// frame to be drawn. This is the "dirty flag" — see section 4.2 for
    /// why this matters for performance.
    pub redraw: bool,
}
impl UpdateOutcome {
    pub fn redraw_only(redraw: bool) -> Self {
        Self {
            commands: Vec::new(),
            redraw,
        }
    }
}
