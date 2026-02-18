//! Confidence scoring for event correlation.

use crate::domain::project::Confidence;

/// Compute the confidence level for a correlation based on matching signals.
///
/// Rules (highest wins):
/// - `sha_matched` -> `Exact`
/// - `timestamp_matched` + `has_explicit_mapping` -> `High`
/// - `timestamp_matched` only -> `Low`
/// - nothing -> `Unmatched`
pub fn score_confidence(
    sha_matched: bool,
    timestamp_matched: bool,
    has_explicit_mapping: bool,
) -> Confidence {
    if sha_matched {
        Confidence::Exact
    } else if timestamp_matched && has_explicit_mapping {
        Confidence::High
    } else if timestamp_matched {
        Confidence::Low
    } else {
        Confidence::Unmatched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_match_is_exact() {
        assert_eq!(
            score_confidence(true, false, false),
            Confidence::Exact
        );
    }

    #[test]
    fn sha_match_overrides_timestamp() {
        assert_eq!(
            score_confidence(true, true, true),
            Confidence::Exact
        );
    }

    #[test]
    fn timestamp_with_mapping_is_high() {
        assert_eq!(
            score_confidence(false, true, true),
            Confidence::High
        );
    }

    #[test]
    fn timestamp_only_is_low() {
        assert_eq!(
            score_confidence(false, true, false),
            Confidence::Low
        );
    }

    #[test]
    fn nothing_is_unmatched() {
        assert_eq!(
            score_confidence(false, false, false),
            Confidence::Unmatched
        );
    }
}
