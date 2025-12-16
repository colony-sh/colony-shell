//! Source span tracking for error reporting and tooling

/// Position in source code
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// Byte offset in source
    pub offset: usize,
    /// 1-indexed line number
    pub line: u32,
    /// 1-indexed column (Unicode scalars, not bytes)
    pub column: u32,
}

impl Position {
    pub const fn new(offset: usize, line: u32, column: u32) -> Self {
        Self {
            offset,
            line,
            column,
        }
    }

    pub const fn zero() -> Self {
        Self {
            offset: 0,
            line: 1,
            column: 1,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::zero()
    }
}

/// Source span (start..end)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
    /// For multi-file tracking
    pub source_id: Option<usize>,
}

impl Span {
    pub const fn new(start: Position, end: Position) -> Self {
        Self {
            start,
            end,
            source_id: None,
        }
    }

    pub const fn with_source_id(start: Position, end: Position, source_id: usize) -> Self {
        Self {
            start,
            end,
            source_id: Some(source_id),
        }
    }

    pub const fn point(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
            source_id: None,
        }
    }

    /// Dummy span for synthetic nodes
    pub const DUMMY: Self = Self {
        start: Position {
            offset: 0,
            line: 0,
            column: 0,
        },
        end: Position {
            offset: 0,
            line: 0,
            column: 0,
        },
        source_id: None,
    };

    /// Combine two spans (start of first to end of second)
    pub fn combine(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end,
            source_id: self.source_id,
        }
    }
}

impl Default for Span {
    fn default() -> Self {
        Self::DUMMY
    }
}
