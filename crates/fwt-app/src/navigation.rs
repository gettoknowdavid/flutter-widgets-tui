//! Navigation stack — a minimal, pure LIFO stack of `Screen`s.
//!
//! Epic 1 scope: only `Screen::Shell` exists. Epic 2+ will add
//! `Screen::Catalog`, `Screen::Detail(WidgetId)`, etc. This type must
//! remain a plain data structure — no `ratatui` types, no I/O — so it can
//! be constructed and tested from `fwt-app` alone, with no terminal and
//! no async runtime.

/// A logical screen/route in the application's navigation history.
///
/// Deliberately NOT a rendering concept — this is the domain-level
/// "where are we" state that `fwt-tui`'s view layer reads to decide what
/// to render, not a Ratatui widget itself. Do not add `ratatui` types to
/// this enum's variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Shell,
}

/// A simple LIFO navigation history stack, backing the breadcrumb trail
/// (TRD Section 8.1) and Backspace/Ctrl+O history traversal (MVP feature
/// list). Real breadcrumb *rendering* logic lands in Epic 3; this ticket
/// only owns the underlying data structure, since it's foundational.
///
/// All operations are panic-free on an empty stack — `pop`/`current`
/// return `None` rather than panicking, since a user pressing "back" one
/// too many times is an entirely ordinary interaction, not a programmer
/// error.
#[derive(Debug, Clone, Default)]
pub struct NavigationStack {
    stack: Vec<Screen>,
}
impl NavigationStack {
    #[must_use]
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Pushes a new screen onto the top of the stack.
    pub fn push(&mut self, screen: Screen) {
        self.stack.push(screen);
    }

    /// Pops the top screen off the stack, returning it.
    ///
    /// Returns `None` on an empty stack rather than panicking.
    pub fn pop(&mut self) -> Option<Screen> {
        self.stack.pop()
    }

    /// Returns a reference to the current (topmost) screen, if any.
    pub fn current(&self) -> Option<&Screen> {
        self.stack.last()
    }

    /// Returns `true` if the stack has no screens at all.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Number of screens currently on the stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn new_stack_is_empty() {
        let stack = NavigationStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
    }

    #[test]
    pub fn current_on_empty_stack_returns_none() {
        let stack = NavigationStack::new();
        assert_eq!(stack.current(), None);
    }

    #[test]
    pub fn pop_on_empty_stack_returns_none_without_panicking() {
        let mut stack = NavigationStack::new();
        assert_eq!(stack.pop(), None);
        assert_eq!(stack.pop(), None);
    }

    #[test]
    pub fn push_then_current_returns_pushed_value() {
        let mut stack = NavigationStack::new();
        stack.push(Screen::Shell);
        assert_eq!(stack.current(), Some(&Screen::Shell));
    }

    #[test]
    pub fn push_pop_push_current_behaves_as_lifo() {
        let mut stack = NavigationStack::new();
        stack.push(Screen::Shell);
        assert_eq!(stack.pop(), Some(Screen::Shell));
        assert_eq!(stack.current(), None);

        stack.push(Screen::Shell);
        stack.push(Screen::Shell);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.pop(), Some(Screen::Shell));
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.current(), Some(&Screen::Shell));
    }
}
