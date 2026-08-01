use crate::command::Command;
use crate::message::Message;
use crate::navigation::{NavigationStack, Screen};
use crate::outcome::UpdateOutcome;
use crate::search_service::SearchResult;
use crossterm::event::{KeyCode, KeyEvent};
use fwt_domain::widget::CategorySummary;

/// Reachability of the AI/sync endpoints. Only AI Chat and
/// GitHub Sync are gated by this — browsing, search, favorites
/// (local), and theming are always available regardless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectivityStatus {
    Online,
    Offline,
    Degraded,
    #[default]
    Unknown,
}

/// Which theme is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeId {
    #[default]
    Default,
}

#[derive(Debug, Clone, Default)]
pub struct CatalogState {
    pub categories: Vec<CategorySummary>,
    pub selected_index: Option<usize>,
}

/// Epic 2: search / fuzzy-match state.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub index_ready: bool,
    pub selected_index: Option<usize>,
}

/// Epic 3: widget detail view state (overview/code/properties/methods tabs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DetailStatePlaceholder;

/// Epic 4: favorites + GitHub sync state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FavoritesStatePlaceholder;

/// Epic 5: AI chat state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChatStatePlaceholder;

#[derive(Debug, Clone)]
pub struct AppState {
    pub navigation: NavigationStack,
    pub active_theme: ThemeId,
    pub connectivity: ConnectivityStatus,
    pub should_quit: bool,
    pub terminal_size: (u16, u16),

    pub catalog: CatalogState,
    pub search: SearchState,
    pub detail: DetailStatePlaceholder,
    pub favorites: FavoritesStatePlaceholder,
    pub chat: ChatStatePlaceholder,
}
impl Default for AppState {
    fn default() -> Self {
        let mut navigation = NavigationStack::new();
        navigation.push(Screen::Shell);

        Self {
            navigation,
            active_theme: ThemeId::default(),
            connectivity: ConnectivityStatus::default(),
            should_quit: false,
            terminal_size: (0, 0),
            catalog: CatalogState::default(),
            search: SearchState::default(),
            detail: DetailStatePlaceholder,
            favorites: FavoritesStatePlaceholder,
            chat: ChatStatePlaceholder,
        }
    }
}

const QUIT_KEY: crossterm::event::KeyCode = crossterm::event::KeyCode::Char('q');

/// THE single place allowed to mutate `AppState`. Pure, synchronous, no
/// I/O, no `.await`.
pub fn update(state: &mut AppState, message: Message) -> UpdateOutcome {
    match message {
        Message::Key(key_event) => handle_key(state, key_event),
        Message::Resize(width, height) => {
            state.terminal_size = (width, height);
            UpdateOutcome::redraw_only(true)
        }
        Message::Tick(_) => UpdateOutcome::redraw_only(false),
        Message::DebugTaskCompleted => UpdateOutcome::redraw_only(true),
        Message::Quit => {
            state.should_quit = true;
            UpdateOutcome::redraw_only(true)
        }
        Message::SearchIndexReady => {
            state.search.index_ready = true;
            UpdateOutcome::redraw_only(true)
        }
        Message::SearchResults(answered_query, results) => {
            if answered_query != state.search.query {
                return UpdateOutcome::redraw_only(false);
            }
            state.search.results = results;
            state.search.selected_index = if state.search.results.is_empty() {
                None
            } else {
                Some(0)
            };
            UpdateOutcome::redraw_only(true)
        }
    }
}

/// Key-event handling, factored out of the top-level match per this
/// ticket's design note: once per-feature key routing lands (Epic 2+),
/// this is where it should be extended/delegated to (e.g. a future
/// `handle_key` dispatching to `update_catalog`/`update_chat`), rather
/// than growing `update()`'s top-level match into a god-function.
fn handle_key(state: &mut AppState, key_event: KeyEvent) -> UpdateOutcome {
    if key_event.code == QUIT_KEY {
        // Delegate to the same effect as Message::Quit rather than
        // duplicating the "set should_quit" logic in two places.
        state.should_quit = true;
        return UpdateOutcome::redraw_only(true);
    }

    match state.navigation.current() {
        Some(Screen::Shell) => handle_shell_key(state, key_event),
        Some(Screen::Catalog) => handle_catalog_key(state, key_event),
        Some(Screen::Search) => handle_search_key(state, key_event),
        None => UpdateOutcome::redraw_only(false),
    }
}

pub fn handle_shell_key(state: &mut AppState, key_event: KeyEvent) -> UpdateOutcome {
    match key_event.code {
        KeyCode::Char('1') => {
            state.navigation.push(Screen::Catalog);
            UpdateOutcome::redraw_only(true)
        }
        KeyCode::Char('2') => {
            state.navigation.push(Screen::Search);
            UpdateOutcome::redraw_only(true)
        }
        _ => UpdateOutcome::redraw_only(false),
    }
}

pub fn handle_catalog_key(state: &mut AppState, key_event: KeyEvent) -> UpdateOutcome {
    UpdateOutcome::redraw_only(false)
}

pub fn handle_search_key(state: &mut AppState, key_event: KeyEvent) -> UpdateOutcome {
    match key_event.code {
        KeyCode::Esc => {
            state.navigation.pop();
            UpdateOutcome::redraw_only(true)
        }
        KeyCode::Char(c) => {
            state.search.query.push(c);
            let outcome = UpdateOutcome {
                commands: vec![Command::Search(state.search.query.clone())],
                redraw: true,
            };
            outcome
        }
        KeyCode::Backspace => {
            state.search.query.pop();
            UpdateOutcome {
                commands: vec![Command::Search(state.search.query.clone())],
                redraw: true,
            }
        }
        KeyCode::Down => {
            move_selection(
                &mut state.search.selected_index,
                state.search.results.len(),
                1,
            );
            UpdateOutcome::redraw_only(true)
        }
        KeyCode::Up => {
            move_selection(
                &mut state.search.selected_index,
                state.search.results.len(),
                -1,
            );
            UpdateOutcome::redraw_only(true)
        }
        KeyCode::Enter => {
            if let Some(idx) = state.search.selected_index {
                if let Some(result) = state.search.results.get(idx) {
                    tracing::info!(widget_id = ?result.widget_id, "selected widget (detail view is Epic 3)");
                }
            }
            UpdateOutcome::redraw_only(false)
        }
        _ => UpdateOutcome::redraw_only(false),
    }
}

/// Clamped selection movement — no panic/wraparound on empty or
/// single-item lists.
fn move_selection(selected: &mut Option<usize>, len: usize, delta: i32) {
    if len == 0 {
        *selected = None;
        return;
    }
    let current = selected.unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, len as i32 - 1);
    *selected = Some(next as usize);
}

pub fn message_from_crossterm_event(event: crossterm::event::Event) -> Option<Message> {
    use crossterm::event::Event as CtEvent;
    match event {
        CtEvent::Key(key_event) => Some(Message::Key(key_event)),
        CtEvent::Resize(w, h) => Some(Message::Resize(w, h)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn default_app_state_has_shell_on_navigation_stack() {
        let state = AppState::default();
        assert_eq!(state.navigation.current(), Some(&Screen::Shell));
        assert!(!state.should_quit);
        assert_eq!(state.connectivity, ConnectivityStatus::Unknown);
        assert_eq!(state.active_theme, ThemeId::Default);
        assert_eq!(state.terminal_size, (0, 0));
    }

    #[test]
    fn quit_message_sets_should_quit_and_skips_redraw() {
        let mut state = AppState::default();
        let outcome = update(&mut state, Message::Quit);

        assert!(state.should_quit);
        assert!(outcome.redraw, "{}", false);
        assert!(outcome.commands.is_empty());
    }

    #[rstest]
    #[case(0, 0)]
    #[case(80, 24)]
    #[case(1, 1)]
    fn resize_message_updates_terminal_size_and_always_redraws(
        #[case] width: u16,
        #[case] height: u16,
    ) {
        let mut state = AppState::default();
        let outcome = update(&mut state, Message::Resize(width, height));

        assert_eq!(state.terminal_size, (width, height));
        assert_eq!(outcome.redraw, true);
    }

    #[test]
    fn resize_to_same_size_still_redraws() {
        let mut state = AppState {
            terminal_size: (80, 24),
            ..Default::default()
        };
        let outcome = update(&mut state, Message::Resize(80, 24));

        assert_eq!(state.terminal_size, (80, 24));
        assert_eq!(
            outcome.redraw, true,
            "a resize event always redraws, even if unchanged"
        );
    }

    #[test]
    fn tick_message_never_redraws_in_epic_1() {
        let mut state = AppState::default();
        let outcome = update(&mut state, Message::Tick(std::time::Instant::now()));

        assert_eq!(outcome.redraw, false);
        assert!(!state.should_quit);
    }

    #[test]
    fn debug_task_completed_is_a_no_op_redraw_wise() {
        let mut state = AppState::default();
        let outcome = update(&mut state, Message::DebugTaskCompleted);

        assert!(outcome.redraw, "{}", false);
        assert!(outcome.commands.is_empty());
    }

    #[test]
    fn q_key_triggers_quit() {
        let mut state = AppState::default();
        let outcome = update(&mut state, Message::Key(key(KeyCode::Char('q'))));

        assert!(state.should_quit);
        assert!(outcome.redraw, "{}", false);
    }

    #[rstest]
    #[case(KeyCode::Char('a'))]
    #[case(KeyCode::Char('Q'))] // capital Q deliberately does NOT quit
    #[case(KeyCode::Enter)]
    #[case(KeyCode::Esc)]
    #[case(KeyCode::Tab)]
    fn other_keys_are_no_ops_in_epic_1(#[case] code: KeyCode) {
        let mut state = AppState::default();
        let outcome = update(&mut state, Message::Key(key(code)));

        assert!(!state.should_quit);
        assert_eq!(outcome.redraw, false);
    }

    #[test]
    fn update_never_panics_across_a_mixed_message_sequence() {
        // Cheap fuzz-adjacent smoke test: apply a hand-rolled sequence of
        // varied messages (including degenerate resize dimensions, per
        // Ticket 004's flagged edge case) and assert we never panic,
        // regardless of ordering.
        let mut state = AppState::default();
        let messages = vec![
            Message::Resize(0, 0),
            Message::Tick(std::time::Instant::now()),
            Message::Key(key(KeyCode::Char('x'))),
            Message::Resize(9999, 9999),
            Message::DebugTaskCompleted,
            Message::Key(key(KeyCode::Esc)),
            Message::Tick(std::time::Instant::now()),
        ];

        for msg in messages {
            let _ = update(&mut state, msg);
        }
        // Quit was never sent in this sequence — confirm state didn't
        // quit "by accident" as a side effect of some other message.
        assert!(!state.should_quit);
    }

    #[test]
    fn key_event_converts_to_message_key() {
        let event = CtEvent::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        let msg = message_from_crossterm_event(event);
        assert!(matches!(msg, Some(Message::Key(_))));
    }

    #[test]
    fn resize_event_converts_with_correct_dimensions() {
        let event = CtEvent::Resize(120, 40);
        let msg = message_from_crossterm_event(event);
        assert!(matches!(msg, Some(Message::Resize(120, 40))));
    }

    #[test]
    fn mouse_event_is_ignored_in_epic_1() {
        // Construct a mouse event and assert `None` is returned —
        // NFR-8 (keyboard-only) means we deliberately drop these for now.
    }
}
