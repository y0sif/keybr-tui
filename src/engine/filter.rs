use std::collections::HashSet;

/// Controls which characters the word generator is allowed to use.
///
/// The `focused` key, if set, MUST appear in every generated word.
/// This is the weakest key the user is currently practicing.
pub struct LetterFilter {
    /// Set of characters allowed in generation.
    pub allowed: HashSet<char>,
    /// The key that MUST appear in every word (weakest key).
    pub focused: Option<char>,
}

impl LetterFilter {
    /// Create a filter from a slice of allowed characters and an optional focus key.
    pub fn new(allowed: &[char], focused: Option<char>) -> Self {
        Self {
            allowed: allowed.iter().copied().collect(),
            focused,
        }
    }

    /// Check whether a character is allowed by this filter.
    #[inline]
    pub fn is_allowed(&self, c: char) -> bool {
        c == ' ' || self.allowed.contains(&c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_is_always_allowed() {
        let filter = LetterFilter::new(&['a', 'b'], None);
        assert!(filter.is_allowed(' '));
    }

    #[test]
    fn allowed_chars_pass() {
        let filter = LetterFilter::new(&['e', 't', 'a'], None);
        assert!(filter.is_allowed('e'));
        assert!(filter.is_allowed('t'));
        assert!(!filter.is_allowed('z'));
    }

    #[test]
    fn focused_is_stored() {
        let filter = LetterFilter::new(&['e', 't'], Some('t'));
        assert_eq!(filter.focused, Some('t'));
    }
}
