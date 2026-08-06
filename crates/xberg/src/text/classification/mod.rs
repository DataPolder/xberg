//! Per-page LLM classification.
//!
//! Walks the rendered `content`, slices it on the page boundary metadata produced
//! during extraction, and asks the configured LLM to assign one or more labels
//! from a fixed vocabulary to each page. Results land on
//! [`ExtractedDocument::page_classifications`](crate::types::ExtractedDocument::page_classifications).
//!
//! Triggered by [`ExtractionConfig::page_classification`](crate::core::config::ExtractionConfig::page_classification);
//! invoked by the Middle-stage post-processor in
//! [`crate::plugins::processor::builtin::classification`].

pub mod chunk_classifier;
pub mod page_classifier;

pub use chunk_classifier::classify_chunks;
pub use page_classifier::{classify_pages, classify_text};

/// Classify a single document (as multiple pages or a single text block).
///
/// Aggregates classifications across all pages in the provided text, returning
/// a combined label set that represents the document as a whole.
///
/// # Arguments
///
/// * `pages` - Slice of page texts to classify. Each page is classified independently
///   using the configured LLM, and results are aggregated.
/// * `config` - Classification configuration including labels and LLM settings.
///
/// # Returns
///
/// A vector of `ClassificationLabel` entries representing the document's overall classification.
///
/// # Errors
///
/// Returns an error if `config.labels` is empty or if LLM calls fail.
///
/// # Example
///
/// ```rust,no_run
/// use xberg::text::classification::classify_document;
/// use xberg::core::config::PageClassificationConfig;
/// use xberg::core::config::LlmConfig;
///
/// # async fn example() -> xberg::Result<()> {
/// let config = PageClassificationConfig {
///     labels: vec!["invoice".to_string(), "memo".to_string()],
///     llm: LlmConfig::default(),
///     prompt_template: None,
///     multi_label: false,
/// };
///
/// let pages = vec!["Page 1 content", "Page 2 content"];
/// let labels = classify_document(&pages, &config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn classify_document(
    pages: &[&str],
    config: &crate::core::config::PageClassificationConfig,
) -> crate::Result<Vec<crate::ClassificationLabel>> {
    if config.labels.is_empty() {
        return Err(crate::XbergError::validation(
            "PageClassificationConfig.labels must contain at least one entry",
        ));
    }

    if pages.is_empty() {
        return Ok(Vec::new());
    }

    let ctx = page_classifier::ClassifyContext::new(config);
    let mut per_page_labels: Vec<Vec<crate::ClassificationLabel>> = Vec::new();

    for page_text in pages {
        if page_text.is_empty() {
            continue;
        }
        // NOTE: `classify_one`'s `LlmUsage` is intentionally dropped here.
        // `classify_document` returns a bare `Vec<ClassificationLabel>` with
        // no slot to carry usage, and the only caller (`enrich::enrich`) is
        // outside this fix's file scope — see the #265 write-up for the
        // proposed shape change.
        let (labels, _usage) = page_classifier::classify_one(page_text, &ctx, config).await?;
        per_page_labels.push(labels);
    }

    let aggregated = aggregate_page_labels(&per_page_labels);

    if config.multi_label {
        let mut labels = aggregated;
        labels.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(labels)
    } else {
        let best = aggregated.into_iter().max_by(|a, b| {
            let a_score = a.confidence.unwrap_or(0.0);
            let b_score = b.confidence.unwrap_or(0.0);
            a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(best.into_iter().collect())
    }
}

/// Aggregate per-page classification labels into one label set, averaging
/// confidence across every page that reported the same label instead of
/// keeping an arbitrary single page's score (#265).
///
/// A label's confidence is the mean of the confidences reported for it by
/// pages that included one; if no page reported a confidence for a label, the
/// aggregated confidence is `None`. Labels are returned in first-seen order
/// across `per_page_labels`.
fn aggregate_page_labels(per_page_labels: &[Vec<crate::ClassificationLabel>]) -> Vec<crate::ClassificationLabel> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: std::collections::HashMap<String, (f32, u32)> = std::collections::HashMap::new();

    for labels in per_page_labels {
        for label in labels {
            let entry = counts.entry(label.label.clone()).or_insert_with(|| {
                order.push(label.label.clone());
                (0.0, 0)
            });
            if let Some(conf) = label.confidence {
                entry.0 += conf;
                entry.1 += 1;
            }
        }
    }

    order
        .into_iter()
        .map(|label| {
            let (sum, count) = counts[&label];
            let confidence = if count > 0 { Some(sum / count as f32) } else { None };
            crate::ClassificationLabel { label, confidence }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClassificationLabel;

    fn label(name: &str, confidence: Option<f32>) -> ClassificationLabel {
        ClassificationLabel {
            label: name.to_string(),
            confidence,
        }
    }

    #[test]
    fn should_average_confidence_across_pages_reporting_the_same_label() {
        let per_page = vec![
            vec![label("invoice", Some(0.6))],
            vec![label("invoice", Some(0.8)), label("memo", Some(0.2))],
        ];

        let aggregated = aggregate_page_labels(&per_page);

        assert_eq!(aggregated.len(), 2);
        assert_eq!(aggregated[0].label, "invoice");
        // (0.6_f32 + 0.8_f32) / 2.0 — exact f32 arithmetic result, not the
        // mathematically rounded 0.7 (f32 cannot represent it exactly).
        assert_eq!(aggregated[0].confidence, Some(0.700_000_05));
        assert_eq!(aggregated[1].label, "memo");
        assert_eq!(aggregated[1].confidence, Some(0.2));
    }

    #[test]
    fn should_return_none_confidence_when_no_page_reported_one() {
        let per_page = vec![vec![label("invoice", None)], vec![label("invoice", None)]];

        let aggregated = aggregate_page_labels(&per_page);

        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].label, "invoice");
        assert_eq!(aggregated[0].confidence, None);
    }

    #[test]
    fn should_average_only_over_pages_that_reported_a_confidence() {
        let per_page = vec![vec![label("invoice", Some(0.9))], vec![label("invoice", None)]];

        let aggregated = aggregate_page_labels(&per_page);

        assert_eq!(aggregated.len(), 1);
        assert_eq!(aggregated[0].confidence, Some(0.9), "the None entry must not dilute the average");
    }

    #[test]
    fn should_preserve_first_seen_order() {
        let per_page = vec![vec![label("zeta", Some(0.5)), label("alpha", Some(0.5))]];

        let aggregated = aggregate_page_labels(&per_page);

        let names: Vec<&str> = aggregated.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(names, vec!["zeta", "alpha"]);
    }
}
