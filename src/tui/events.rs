//! Keyboard input mapping (pure, no I/O).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::operations::playback::Action;

/// A command produced by a key press.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiCommand {
    /// A playback action handled by `operations::playback::apply`.
    Playback(Action),
    /// Show episode/player info in the status line.
    Info,
    /// Download the current episode for offline listening.
    Download,
}

/// Maps a key event to a command. Only key presses count (no repeats or
/// releases); other modifiers than Shift disable the plain-letter bindings.
pub fn map_key(key: KeyEvent) -> Option<UiCommand> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(UiCommand::Playback(Action::Quit)),
            _ => None,
        };
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    let KeyCode::Char(c) = key.code else {
        return None;
    };
    Some(match c {
        'n' => UiCommand::Playback(Action::Next),
        'b' => UiCommand::Playback(Action::Previous),
        'p' => UiCommand::Playback(Action::PlayPause),
        's' => UiCommand::Playback(Action::ToggleShuffle),
        'f' => UiCommand::Playback(Action::ToggleFavorite),
        'm' => UiCommand::Playback(Action::ToggleMute),
        '+' => UiCommand::Playback(Action::VolumeUp),
        '-' => UiCommand::Playback(Action::VolumeDown),
        'i' => UiCommand::Info,
        'd' => UiCommand::Download,
        'q' => UiCommand::Playback(Action::Quit),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn key(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        }
    }

    fn press(c: char) -> KeyEvent {
        key(KeyCode::Char(c), KeyModifiers::NONE, KeyEventKind::Press)
    }

    #[test]
    fn single_keys_map_to_commands() {
        let expected = [
            ('n', UiCommand::Playback(Action::Next)),
            ('b', UiCommand::Playback(Action::Previous)),
            ('p', UiCommand::Playback(Action::PlayPause)),
            ('s', UiCommand::Playback(Action::ToggleShuffle)),
            ('f', UiCommand::Playback(Action::ToggleFavorite)),
            ('m', UiCommand::Playback(Action::ToggleMute)),
            ('+', UiCommand::Playback(Action::VolumeUp)),
            ('-', UiCommand::Playback(Action::VolumeDown)),
            ('i', UiCommand::Info),
            ('d', UiCommand::Download),
            ('q', UiCommand::Playback(Action::Quit)),
        ];
        for (c, cmd) in expected {
            assert_eq!(map_key(press(c)), Some(cmd), "key {c}");
        }
        assert_eq!(map_key(press('x')), None);
        assert_eq!(
            map_key(key(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Press)),
            None
        );
    }

    #[test]
    fn plus_with_shift_still_maps() {
        let k = key(KeyCode::Char('+'), KeyModifiers::SHIFT, KeyEventKind::Press);
        assert_eq!(map_key(k), Some(UiCommand::Playback(Action::VolumeUp)));
    }

    #[test]
    fn ctrl_c_quits_and_other_ctrl_keys_are_ignored() {
        let quit = key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(map_key(quit), Some(UiCommand::Playback(Action::Quit)));
        let ctrl_n = key(
            KeyCode::Char('n'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(map_key(ctrl_n), None);
        let alt_n = key(KeyCode::Char('n'), KeyModifiers::ALT, KeyEventKind::Press);
        assert_eq!(map_key(alt_n), None);
    }

    #[test]
    fn only_press_events_count() {
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            assert_eq!(
                map_key(key(KeyCode::Char('n'), KeyModifiers::NONE, kind)),
                None
            );
            assert_eq!(
                map_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL, kind)),
                None
            );
        }
    }
}
