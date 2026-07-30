//! Shared helpers for surfacing degradation to callers without flooding
//! `processing_warnings`.
//!
//! External dependencies (OCR engines, layout/model runtimes, archive readers)
//! commonly emit the *same* warning for every page or every item. Collecting
//! them verbatim buries the signal under duplicates. These helpers collapse
//! identical `(source, message)` warnings to a single entry so a caller sees
//! one line per distinct problem, not one per page — generalizing the per-page
//! dedup introduced for the paddle-ocr language warning (#1346).

#[cfg(all(
    feature = "pdf",
    any(feature = "ocr", feature = "ocr-pipeline", feature = "layout-detection")
))]
use crate::types::ProcessingWarning;

/// Push `warning` into `accumulated` unless an identical `(source, message)`
/// entry is already present. Order of first occurrence is preserved.
#[cfg(all(
    feature = "pdf",
    any(feature = "ocr", feature = "ocr-pipeline", feature = "layout-detection")
))]
pub(crate) fn push_warning_deduped(accumulated: &mut Vec<ProcessingWarning>, warning: ProcessingWarning) {
    if !accumulated
        .iter()
        .any(|existing| existing.source == warning.source && existing.message == warning.message)
    {
        accumulated.push(warning);
    }
}

/// Append `new` warnings into `accumulated`, skipping any whose
/// `(source, message)` pair already appears.
///
/// A backend that warns about its configuration (e.g. paddle-ocr's unsupported
/// language warning, or a per-page model degradation) emits the same warning on
/// every page; one copy per document is enough.
#[cfg(all(feature = "pdf", any(feature = "ocr", feature = "ocr-pipeline")))]
pub(crate) fn dedup_extend_warnings(accumulated: &mut Vec<ProcessingWarning>, new: Vec<ProcessingWarning>) {
    for warning in new {
        push_warning_deduped(accumulated, warning);
    }
}

#[cfg(all(test, feature = "pdf", any(feature = "ocr", feature = "ocr-pipeline")))]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn warning(source: &'static str, message: &str) -> ProcessingWarning {
        ProcessingWarning {
            source: Cow::Borrowed(source),
            message: Cow::Owned(message.to_string()),
        }
    }

    #[test]
    fn dedup_extend_drops_identical_keeps_distinct() {
        let mut accumulated = vec![warning("paddle-ocr", "a")];
        dedup_extend_warnings(
            &mut accumulated,
            vec![warning("paddle-ocr", "a"), warning("paddle-ocr", "b")],
        );
        dedup_extend_warnings(
            &mut accumulated,
            vec![warning("paddle-ocr", "a"), warning("paddle-ocr", "b")],
        );

        let messages: Vec<&str> = accumulated.iter().map(|w| w.message.as_ref()).collect();
        assert_eq!(messages, vec!["a", "b"]);
    }

    #[test]
    fn same_message_different_source_is_kept() {
        let mut accumulated = vec![warning("layout", "failed")];
        push_warning_deduped(&mut accumulated, warning("ocr", "failed"));
        push_warning_deduped(&mut accumulated, warning("layout", "failed"));

        assert_eq!(accumulated.len(), 2, "distinct sources must not collapse");
    }
}
