//! Actions that can be triggered by keybindings

/// Actions that can be performed in the TUI
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Mode changes
    EnterNormal,
    EnterInsert,
    EnterInsertAfter,
    EnterInsertStart,
    EnterInsertEnd,
    EnterCommand,
    EnterSearch,
    InsertLineBelow,
    InsertLineAbove,

    // Text manipulation
    InsertChar(char),
    DeleteChar,
    DeleteCharBackward,
    DeleteWordBackward,
    DeleteToLineStart,
    DeleteToLineEnd,
    DeletePending,

    // Cursor movement
    CursorLeft,
    CursorRight,
    CursorUp,
    CursorDown,
    CursorLineStart,
    CursorLineEnd,
    CursorWordForward,
    CursorWordBackward,
    CursorWordEnd,

    // Scrolling content area
    ScrollUp,
    ScrollDown,
    ScrollHalfPageUp,
    ScrollHalfPageDown,
    ScrollToTop,
    ScrollToBottom,

    // History
    HistoryPrev,
    HistoryNext,
    HistorySearchActivate,
    HistorySearchAccept,
    HistorySearchCancel,
    HistorySearchNext,
    HistorySearchPrev,

    // Completion
    AcceptCompletion,
    NextCompletion,
    PrevCompletion,

    // Clipboard
    YankPending,
    PasteAfter,
    PasteBefore,

    // Undo/Redo
    Undo,
    Redo,

    // Search
    ExecuteSearch,
    SearchNext,
    SearchPrev,

    // Execution
    Execute,
    ExecuteCommand,

    // Global
    Interrupt,
    Eof,
    ToggleLeftSidebar,
    ToggleRightSidebar,
    Quit,

    // Command palette
    CommandPaletteActivate,
    CommandPaletteAccept,
    CommandPaletteCancel,
    CommandPaletteNext,
    CommandPalettePrev,
}
