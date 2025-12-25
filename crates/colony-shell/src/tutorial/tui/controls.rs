use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};

pub enum TutorialAction {
    TogglePause,
    Step,
    ToggleDepth,
    MarkLine,
    Search,
    ToggleTeachMode,
    Quit,
    HistoryBack,
    HistoryForward,
    SpeedUp,
    SpeedDown,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ToggleAstExpand,
    None,
}

pub fn handle_key_event(event: KeyEvent) -> TutorialAction {
    match event.code {
        KeyCode::Char(' ') => TutorialAction::TogglePause,
        KeyCode::Enter => TutorialAction::Step,
        KeyCode::Char('d') => TutorialAction::ToggleDepth,
        KeyCode::Char('m') => TutorialAction::MarkLine,
        KeyCode::Char('/') => TutorialAction::Search,
        KeyCode::Char('t') => TutorialAction::ToggleTeachMode,
        KeyCode::Char('q') => TutorialAction::Quit,
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
            TutorialAction::Quit
        }
        KeyCode::Esc => TutorialAction::Quit,
        KeyCode::Left => TutorialAction::HistoryBack,
        KeyCode::Right => TutorialAction::HistoryForward,
        KeyCode::Char('+') | KeyCode::Char('=') => TutorialAction::SpeedUp,
        KeyCode::Char('-') | KeyCode::Char('_') => TutorialAction::SpeedDown,
        KeyCode::Up | KeyCode::Char('k') => TutorialAction::ScrollUp,
        KeyCode::Down | KeyCode::Char('j') => TutorialAction::ScrollDown,
        KeyCode::PageUp => TutorialAction::ScrollPageUp,
        KeyCode::PageDown => TutorialAction::ScrollPageDown,
        KeyCode::Char('e') => TutorialAction::ToggleAstExpand,
        _ => TutorialAction::None,
    }
}

pub fn handle_mouse_event(_event: MouseEvent) -> TutorialAction {
    TutorialAction::None
}

pub fn handle_event(event: Event) -> TutorialAction {
    match event {
        Event::Key(key) => handle_key_event(key),
        Event::Mouse(mouse) => handle_mouse_event(mouse),
        _ => TutorialAction::None,
    }
}
