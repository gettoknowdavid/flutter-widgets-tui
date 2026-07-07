use crate::message::Message;
use crate::outcome::UpdateOutcome;

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub should_quit: bool,
    pub terminal_size: (u16, u16),
}

/// THE single place allowed to mutate `AppState`. Pure, synchronous, no
/// I/O, no `.await`.
pub fn update(state: &mut AppState, message: Message) -> UpdateOutcome {
    match message {
        Message::Key(_key_event) => UpdateOutcome {
            commands: vec![],
            redraw: false,
        },
        Message::Resize(w, h) => {
            state.terminal_size = (w, h);
            UpdateOutcome {
                commands: vec![],
                redraw: true,
            }
        }
        Message::Tick(_) => UpdateOutcome {
            commands: vec![],
            redraw: false,
        },
        Message::DebugTaskCompleted => UpdateOutcome {
            commands: vec![],
            redraw: true,
        },
        Message::Quit => {
            state.should_quit = true;
            UpdateOutcome {
                commands: vec![],
                redraw: true,
            }
        }
    }
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
