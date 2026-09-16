//! Unit tests for the constants module.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]

/// Test module for constants validation.
mod constants_test {
    use crate::utils::constants::MAX_WINDOW_TITLE_LENGTH;

    #[test]
    fn test_max_window_title_length_is_nonzero() {
        assert!(MAX_WINDOW_TITLE_LENGTH > 0);
    }
}
