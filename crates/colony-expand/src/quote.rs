//! Quote removal
//!
//! In our architecture, quote removal is handled implicitly:
//! - Quoted segments are marked in IntermediateWord
//! - Field splitting respects quoted boundaries
//! - Final fields don't include quote characters
//!
//! This module exists for completeness but the real work is done
//! in the Field type and split_fields function.

/// Quote removal is implicit in our architecture
///
/// The quotes themselves are not present in the WordPart enum values:
/// - SingleQuoted(s) contains the content without quotes
/// - DoubleQuoted(parts) contains the content without quotes
///
/// So there's nothing to "remove" - we never had the quote characters
/// in the expanded text to begin with.
#[allow(dead_code)]
pub fn remove_quotes(text: &str) -> String {
    // In our architecture, quotes are already removed during parsing
    text.to_string()
}
