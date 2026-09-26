//! Keyboard input mapping (pure, no I/O).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::operations::playback::Action;

use super::app::InputMode;
use super::list::ListMove;

/// A command produced by a key press.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiCommand {
    /// A playback action handled by `operations::playback::apply`.
    Playback(Action),
    /// Show episode/player info in the status line.
    Info,
    /// Download the current episode for offline listening.
    Download,
    /// Hide or show the episode list (compact and full layouts).
    ToggleList,
    /// Next visualizer style (turns the visualizer on if it was off).
    VizNext,
    /// Turn the visualizer on or off.
    VizToggle,
    /// Move the list selection.
    Move(ListMove),
    /// Play the selected episode.
    PlaySelected,
    /// Enter search mode.
    StartSearch,
    /// A character typed into the search query.
    SearchInput(char),
    /// Delete the last character of the search query.
    SearchBackspace,
    /// Keep the filter and go back to list mode.
    SearchAccept,
    /// Clear the filter and go back to list mode.
    SearchCancel,
}

/// Maps a key event to a command. Only key presses count (no repeats or
/// releases); other modifiers than Shift disable the plain-letter bindings.
///
/// In [`InputMode::Search`] every plain character is text for the query, so no
/// playback shortcut can fire while typing; only Ctrl+C still quits.
pub fn map_key(key: KeyEvent, mode: InputMode) -> Option<UiCommand> {
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
    if let Some(movement) = map_movement(key.code) {
        return Some(UiCommand::Move(movement));
    }
    match mode {
        InputMode::Search => match key.code {
            KeyCode::Char(c) => Some(UiCommand::SearchInput(c)),
            KeyCode::Backspace => Some(UiCommand::SearchBackspace),
            KeyCode::Enter => Some(UiCommand::SearchAccept),
            KeyCode::Esc => Some(UiCommand::SearchCancel),
            _ => None,
        },
        InputMode::List => match key.code {
            KeyCode::Enter => Some(UiCommand::PlaySelected),
            KeyCode::Char(c) => map_list_char(c),
            _ => None,
        },
    }
}

/// Navigation keys that work in both modes (no plain letters: those are text
/// while searching).
fn map_movement(code: KeyCode) -> Option<ListMove> {
    match code {
        KeyCode::Up => Some(ListMove::Up),
        KeyCode::Down => Some(ListMove::Down),
        KeyCode::PageUp => Some(ListMove::PageUp),
        KeyCode::PageDown => Some(ListMove::PageDown),
        KeyCode::Home => Some(ListMove::Home),
        KeyCode::End => Some(ListMove::End),
        _ => None,
    }
}

fn map_list_char(c: char) -> Option<UiCommand> {
    Some(match c {
        'j' => UiCommand::Move(ListMove::Down),
        'k' => UiCommand::Move(ListMove::Up),
        'g' => UiCommand::Move(ListMove::Home),
        'G' => UiCommand::Move(ListMove::End),
        '/' => UiCommand::StartSearch,
        'h' => UiCommand::ToggleList,
        'v' => UiCommand::VizNext,
        'V' => UiCommand::VizToggle,
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
            ('h', UiCommand::ToggleList),
            ('v', UiCommand::VizNext),
            ('V', UiCommand::VizToggle),
            ('q', UiCommand::Playback(Action::Quit)),
        ];
        for (c, cmd) in expected {
            assert_eq!(map_key(press(c), InputMode::List), Some(cmd), "key {c}");
        }
        assert_eq!(map_key(press('x'), InputMode::List), None);
        // `w`/`W` are no longer bound.
        assert_eq!(map_key(press('w'), InputMode::List), None);
        assert_eq!(map_key(press('W'), InputMode::List), None);
        assert_eq!(
            map_key(
                key(KeyCode::Tab, KeyModifiers::NONE, KeyEventKind::Press),
                InputMode::List
            ),
            None
        );
    }

    #[test]
    fn plus_with_shift_still_maps() {
        let k = key(KeyCode::Char('+'), KeyModifiers::SHIFT, KeyEventKind::Press);
        assert_eq!(
            map_key(k, InputMode::List),
            Some(UiCommand::Playback(Action::VolumeUp))
        );
    }

    #[test]
    fn ctrl_c_quits_and_other_ctrl_keys_are_ignored() {
        let quit = key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(
            map_key(quit, InputMode::List),
            Some(UiCommand::Playback(Action::Quit))
        );
        let ctrl_n = key(
            KeyCode::Char('n'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(map_key(ctrl_n, InputMode::List), None);
        let alt_n = key(KeyCode::Char('n'), KeyModifiers::ALT, KeyEventKind::Press);
        assert_eq!(map_key(alt_n, InputMode::List), None);
    }

    #[test]
    fn only_press_events_count() {
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            assert_eq!(
                map_key(
                    key(KeyCode::Char('n'), KeyModifiers::NONE, kind),
                    InputMode::List
                ),
                None
            );
            assert_eq!(
                map_key(
                    key(KeyCode::Char('c'), KeyModifiers::CONTROL, kind),
                    InputMode::Search
                ),
                None
            );
        }
    }

    fn code(c: KeyCode) -> KeyEvent {
        key(c, KeyModifiers::NONE, KeyEventKind::Press)
    }

    #[test]
    fn list_mode_navigation_and_view_keys() {
        let m = |c| map_key(code(c), InputMode::List);
        assert_eq!(m(KeyCode::Down), Some(UiCommand::Move(ListMove::Down)));
        assert_eq!(m(KeyCode::Up), Some(UiCommand::Move(ListMove::Up)));
        assert_eq!(m(KeyCode::Char('j')), Some(UiCommand::Move(ListMove::Down)));
        assert_eq!(m(KeyCode::Char('k')), Some(UiCommand::Move(ListMove::Up)));
        assert_eq!(
            m(KeyCode::PageDown),
            Some(UiCommand::Move(ListMove::PageDown))
        );
        assert_eq!(m(KeyCode::PageUp), Some(UiCommand::Move(ListMove::PageUp)));
        assert_eq!(m(KeyCode::Home), Some(UiCommand::Move(ListMove::Home)));
        assert_eq!(m(KeyCode::End), Some(UiCommand::Move(ListMove::End)));
        assert_eq!(m(KeyCode::Char('g')), Some(UiCommand::Move(ListMove::Home)));
        assert_eq!(m(KeyCode::Char('G')), Some(UiCommand::Move(ListMove::End)));
        assert_eq!(m(KeyCode::Enter), Some(UiCommand::PlaySelected));
        assert_eq!(m(KeyCode::Char('/')), Some(UiCommand::StartSearch));
        assert_eq!(m(KeyCode::Char('h')), Some(UiCommand::ToggleList));
        assert_eq!(m(KeyCode::Char('v')), Some(UiCommand::VizNext));
        assert_eq!(m(KeyCode::Esc), None);
    }

    #[test]
    fn search_mode_sends_letters_to_the_input_never_to_playback() {
        for c in [
            'n', 'b', 'p', 's', 'f', 'm', 'i', 'd', 'q', 'h', 'v', 'V', 'j', 'k', 'g', 'G', '/',
            '+', '-', 'w', 'W',
        ] {
            assert_eq!(
                map_key(press(c), InputMode::Search),
                Some(UiCommand::SearchInput(c)),
                "key {c}"
            );
        }
        let shifted = key(KeyCode::Char('N'), KeyModifiers::SHIFT, KeyEventKind::Press);
        assert_eq!(
            map_key(shifted, InputMode::Search),
            Some(UiCommand::SearchInput('N'))
        );
    }

    #[test]
    fn shift_v_toggles_the_visualizer_and_is_text_while_searching() {
        let shift_v = key(KeyCode::Char('V'), KeyModifiers::SHIFT, KeyEventKind::Press);
        assert_eq!(
            map_key(shift_v, InputMode::List),
            Some(UiCommand::VizToggle)
        );
        assert_eq!(
            map_key(shift_v, InputMode::Search),
            Some(UiCommand::SearchInput('V'))
        );
        assert_eq!(
            map_key(press('v'), InputMode::List),
            Some(UiCommand::VizNext)
        );
        assert_eq!(
            map_key(press('v'), InputMode::Search),
            Some(UiCommand::SearchInput('v'))
        );
        assert_eq!(
            map_key(press('h'), InputMode::Search),
            Some(UiCommand::SearchInput('h'))
        );
    }

    #[test]
    fn search_mode_editing_keys_and_navigation() {
        let m = |c| map_key(code(c), InputMode::Search);
        assert_eq!(m(KeyCode::Backspace), Some(UiCommand::SearchBackspace));
        assert_eq!(m(KeyCode::Enter), Some(UiCommand::SearchAccept));
        assert_eq!(m(KeyCode::Esc), Some(UiCommand::SearchCancel));
        assert_eq!(m(KeyCode::Down), Some(UiCommand::Move(ListMove::Down)));
        assert_eq!(m(KeyCode::Tab), None);
    }

    #[test]
    fn ctrl_c_quits_in_search_mode_and_other_ctrl_keys_do_not_type() {
        let quit = key(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(
            map_key(quit, InputMode::Search),
            Some(UiCommand::Playback(Action::Quit))
        );
        let ctrl_n = key(
            KeyCode::Char('n'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        assert_eq!(map_key(ctrl_n, InputMode::Search), None);
    }
}
