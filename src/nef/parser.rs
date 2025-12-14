mod checksum;
mod method_tokens;
mod parse;

/// Parser for Neo N3 NEF containers.
///
/// This type is stateless and can be reused across many parse calls.
#[derive(Debug, Default, Clone, Copy)]
pub struct NefParser;

impl NefParser {
    /// Create a new NEF parser.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}
