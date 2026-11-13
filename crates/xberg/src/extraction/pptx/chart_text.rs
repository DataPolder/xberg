//! Text of a chart part (`ppt/charts/chartN.xml`), bounded (DataPolder X60).
//!
//! A chart caches every data point it plots (`c:v`). Joined in full, a chart
//! of a few hundred thousand logged values became 15 M characters of text and
//! some 80,000 chunks for one slide. A chart of at most
//! [`chart_max_values`] values keeps all of them, as before; a larger one is
//! summarised per series (count, min, max, the first [`SAMPLE_HEAD`] and last
//! [`SAMPLE_TAIL`] values), and its category labels likewise once there are
//! more distinct labels than the limit. Title, axis titles, series names and
//! number formats are always kept.

use std::sync::atomic::{AtomicUsize, Ordering};

use roxmltree::{Document, Node};

use crate::error::{Result, XbergError};
use crate::extraction::ooxml_constants::DRAWINGML_NAMESPACE;
use crate::text::utf8_validation;

const CHART_NAMESPACE: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";

/// Default for [`chart_max_values`].
pub const DEFAULT_CHART_MAX_VALUES: usize = 5_000;

/// Values (and, separately, distinct category labels) kept from the start of
/// a summarised series.
pub const SAMPLE_HEAD: usize = 20;

/// Values kept from the end of a summarised series.
pub const SAMPLE_TAIL: usize = 5;

static CHART_MAX_VALUES: AtomicUsize = AtomicUsize::new(DEFAULT_CHART_MAX_VALUES);

/// Set the most data points a chart may have before its values are
/// summarised. Process-wide: the limit is an installation setting, and a
/// process-wide value keeps it out of every binding's `ExtractionConfig`.
pub fn set_chart_max_values(max_values: usize) {
    CHART_MAX_VALUES.store(max_values, Ordering::Relaxed);
}

/// The most data points a chart may have before its values are summarised.
pub fn chart_max_values() -> usize {
    CHART_MAX_VALUES.load(Ordering::Relaxed)
}

fn is_chart(n: &Node, name: &str) -> bool {
    n.tag_name().namespace() == Some(CHART_NAMESPACE) && n.tag_name().name() == name
}

fn child<'a, 'i>(n: &Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| is_chart(c, name))
}

/// Every non-empty, trimmed `c:v` under `n`, in document order.
fn values_under(n: &Node) -> Vec<String> {
    n.descendants()
        .filter(|d| is_chart(d, "v"))
        .filter_map(|d| d.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The DrawingML text (`a:t`) under a `c:title`, joined.
fn title_text(title: &Node) -> String {
    title
        .descendants()
        .filter(|n| n.tag_name().namespace() == Some(DRAWINGML_NAMESPACE) && n.tag_name().name() == "t")
        .filter_map(|n| n.text())
        .collect::<Vec<_>>()
        .join("")
}

struct Series {
    name: String,
    format: Option<String>,
    values: Vec<String>,
    labels: Vec<String>,
}

fn series_of(ser: &Node) -> Series {
    // A name is one cell, or a few for a multi-level one; a name range of
    // thousands of cells is sampled like the values, so it cannot carry them.
    let name = child(ser, "tx")
        .map(|tx| {
            let parts = values_under(&tx);
            if parts.len() <= SAMPLE_HEAD + SAMPLE_TAIL {
                parts.join(" ")
            } else {
                sampled(&parts, "name parts")
            }
        })
        .unwrap_or_default();
    let mut values = Vec::new();
    let mut format = None;
    for tag in ["val", "yVal", "bubbleSize"] {
        if let Some(v) = child(ser, tag) {
            values.extend(values_under(&v));
            if format.is_none() {
                format = v
                    .descendants()
                    .find(|d| is_chart(d, "formatCode"))
                    .and_then(|d| d.text())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("General"));
            }
        }
    }
    let labels = ["cat", "xVal"]
        .iter()
        .find_map(|t| child(ser, t))
        .map(|c| values_under(&c))
        .unwrap_or_default();
    Series {
        name,
        format,
        values,
        labels,
    }
}

/// `items` in full up to `SAMPLE_HEAD + SAMPLE_TAIL` of them; else the first
/// [`SAMPLE_HEAD`], a `… (M more <what>) …` marker and the last [`SAMPLE_TAIL`].
fn sampled(items: &[String], what: &str) -> String {
    if items.len() <= SAMPLE_HEAD + SAMPLE_TAIL {
        return items.join(", ");
    }
    let more = items.len() - SAMPLE_HEAD - SAMPLE_TAIL;
    format!(
        "{}, … ({more} more {what}) …, {}",
        items[..SAMPLE_HEAD].join(", "),
        items[items.len() - SAMPLE_TAIL..].join(", ")
    )
}

/// One series, summarised: name, format, count, min and max (of the values
/// that read as numbers), and a sample.
fn series_summary(s: &Series) -> String {
    let mut head = if s.name.is_empty() {
        "Series".to_string()
    } else {
        s.name.clone()
    };
    if let Some(f) = &s.format {
        head.push_str(&format!(" ({f})"));
    }
    let numbers: Vec<(f64, &String)> = s
        .values
        .iter()
        .filter_map(|v| v.parse::<f64>().ok().filter(|x| x.is_finite()).map(|x| (x, v)))
        .collect();
    let mut stats = format!("{} values", s.values.len());
    if let (Some(min), Some(max)) = (
        numbers.iter().min_by(|a, b| a.0.total_cmp(&b.0)),
        numbers.iter().max_by(|a, b| a.0.total_cmp(&b.0)),
    ) {
        stats.push_str(&format!(", min {}, max {}", min.1, max.1));
    }
    format!("{head}: {stats}: {}", sampled(&s.values, "values"))
}

/// Category labels of every series, each label once, in first-seen order.
fn distinct_labels(series: &[Series]) -> Vec<String> {
    let mut seen = ahash::AHashSet::new();
    let mut out = Vec::new();
    for s in series {
        for l in &s.labels {
            if seen.insert(l.as_str()) {
                out.push(l.clone());
            }
        }
    }
    out
}

/// Parse the text content of a chart part (e.g. `ppt/charts/chart1.xml`).
///
/// Recovers the chart title and every cached string/numeric value (`c:v`),
/// which together cover series names, category labels, and data points —
/// in full for a chart of at most [`chart_max_values`] values, summarised
/// above that (module docs). Axis titles and non-`General` number formats
/// follow on their own lines. Returns `Ok(None)` when the chart carries no
/// recoverable text.
pub(super) fn parse_chart_text(xml_data: &[u8]) -> Result<Option<String>> {
    parse_chart_text_with(xml_data, chart_max_values())
}

/// [`parse_chart_text`] with an explicit limit (tests set it per call; the
/// process-wide one is shared by every test thread).
fn parse_chart_text_with(xml_data: &[u8], limit: usize) -> Result<Option<String>> {
    let xml_str = utf8_validation::from_utf8(xml_data)
        .map_err(|e| XbergError::parsing(format!("Invalid UTF-8 in chart XML: {}", e)))?;
    let doc = Document::parse(xml_str).map_err(|e| XbergError::parsing(format!("Failed to parse chart XML: {}", e)))?;

    // The first `c:title` in the part, as before: the chart's own title, or,
    // on a chart without one, the first axis title.
    let title_node = doc.descendants().find(|n| is_chart(n, "title"));
    let title = title_node.map(|t| title_text(&t)).filter(|s| !s.trim().is_empty());

    let axis_titles: Vec<String> = doc
        .descendants()
        .filter(|n| is_chart(n, "title"))
        .filter(|n| Some(*n) != title_node)
        .filter(|n| {
            n.parent()
                .is_some_and(|p| ["valAx", "catAx", "dateAx", "serAx"].iter().any(|a| is_chart(&p, a)))
        })
        .map(|n| title_text(&n))
        .filter(|s| !s.trim().is_empty())
        .collect();

    let series: Vec<Series> = doc
        .descendants()
        .filter(|n| is_chart(n, "ser"))
        .map(|s| series_of(&s))
        .collect();
    let data_points: usize = series.iter().map(|s| s.values.len()).sum();

    let mut parts = Vec::new();
    if let Some(t) = title {
        parts.push(t);
    }

    if data_points <= limit {
        let values = values_under(&doc.root());
        if !values.is_empty() {
            parts.push(values.join(", "));
        }
    } else {
        let labels = distinct_labels(&series);
        if !labels.is_empty() {
            let shown = if labels.len() <= limit {
                labels.join(", ")
            } else {
                sampled(&labels, "labels")
            };
            parts.push(format!("Categories ({}): {shown}", labels.len()));
        }
        for s in &series {
            parts.push(series_summary(s));
        }
    }

    if !axis_titles.is_empty() {
        parts.push(format!("Axis titles: {}", axis_titles.join("; ")));
    }
    // Number formats travel in the summary of a large chart already.
    if data_points <= limit {
        let formats: Vec<(&str, &str)> = series
            .iter()
            .filter_map(|s| s.format.as_deref().map(|f| (s.name.as_str(), f)))
            .collect();
        let mut distinct: Vec<&str> = Vec::new();
        for (_, f) in &formats {
            if !distinct.contains(f) {
                distinct.push(f);
            }
        }
        match distinct.as_slice() {
            [] => {}
            [one] if formats.len() == series.len() => parts.push(format!("Number format: {one}")),
            _ => parts.push(format!(
                "Number formats: {}",
                formats
                    .iter()
                    .map(|(n, f)| if n.is_empty() {
                        f.to_string()
                    } else {
                        format!("{n}: {f}")
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
    }

    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parts.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart>"#;
    const TAIL: &str = "</c:chart></c:chartSpace>";

    fn title(t: &str) -> String {
        format!("<c:title><c:tx><c:rich><a:p><a:r><a:t>{t}</a:t></a:r></a:p></c:rich></c:tx></c:title>")
    }

    fn series(name: &str, format: &str, labels: &[String], values: &[String]) -> String {
        let pts = |v: &[String]| {
            v.iter()
                .enumerate()
                .map(|(i, x)| format!(r#"<c:pt idx="{i}"><c:v>{x}</c:v></c:pt>"#))
                .collect::<String>()
        };
        format!(
            r#"<c:ser><c:tx><c:strRef><c:strCache><c:pt idx="0"><c:v>{name}</c:v></c:pt></c:strCache></c:strRef></c:tx><c:cat><c:strRef><c:strCache>{}</c:strCache></c:strRef></c:cat><c:val><c:numRef><c:numCache><c:formatCode>{format}</c:formatCode>{}</c:numCache></c:numRef></c:val></c:ser>"#,
            pts(labels),
            pts(values)
        )
    }

    fn chart(title_xml: &str, plot: &str, axes: &str) -> Vec<u8> {
        format!("{HEAD}{title_xml}<c:plotArea><c:lineChart>{plot}</c:lineChart>{axes}</c:plotArea>{TAIL}").into_bytes()
    }

    fn strings(n: usize, f: impl Fn(usize) -> String) -> Vec<String> {
        (0..n).map(f).collect()
    }

    /// A chart at or under the limit reads as before: the title, then every
    /// cached value in document order. `General` adds no format line.
    #[test]
    fn a_small_chart_keeps_every_value_as_before() {
        let labels = strings(3, |i| format!("Q{}", i + 1));
        let values = strings(3, |i| format!("{}", 40 + i));
        let xml = chart(&title("Revenue"), &series("2026", "General", &labels, &values), "");
        let text = parse_chart_text_with(&xml, 6).unwrap().unwrap();
        assert_eq!(text, "Revenue\n2026, Q1, Q2, Q3, 40, 41, 42");
    }

    /// Axis titles and a non-`General` number format follow, also on a
    /// small chart (EEvers, #4053 Q6).
    #[test]
    fn axis_titles_and_number_format_are_kept() {
        let labels = strings(2, |i| format!("t{i}"));
        let values = strings(2, |i| format!("{i}.5"));
        let axes = format!(
            "<c:catAx>{}</c:catAx><c:valAx>{}</c:valAx>",
            title("Time"),
            title("Temperature (°C)")
        );
        let xml = chart(&title("Oven"), &series("Sensor", "0.0", &labels, &values), &axes);
        let text = parse_chart_text_with(&xml, 100).unwrap().unwrap();
        assert!(
            text.ends_with("\nAxis titles: Time; Temperature (°C)\nNumber format: 0.0"),
            "{text}"
        );
        // Two series with their own formats name each.
        let plot = series("A", "0%", &labels, &values) + &series("B", "#,##0 €", &labels, &values);
        let text = parse_chart_text_with(&chart("", &plot, ""), 100).unwrap().unwrap();
        assert!(text.ends_with("\nNumber formats: A: 0%; B: #,##0 €"), "{text}");
    }

    /// Over the limit: per series the count, min, max and the first 20 and
    /// last 5 values; labels in full while distinct ones fit the limit.
    #[test]
    fn a_large_chart_is_summarised_per_series() {
        let labels = strings(30, |i| format!("c{}", i % 3));
        let a = strings(30, |i| format!("{}", i * 2));
        let b = strings(30, |i| format!("{}", 100 - i));
        let plot = series("Sensor A", "0", &labels, &a) + &series("Sensor B", "General", &labels, &b);
        let text = parse_chart_text_with(&chart(&title("Run"), &plot, ""), 59)
            .unwrap()
            .unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Run");
        assert_eq!(lines[1], "Categories (3): c0, c1, c2");
        let head_a = strings(20, |i| format!("{}", i * 2)).join(", ");
        assert_eq!(
            lines[2],
            format!("Sensor A (0): 30 values, min 0, max 58: {head_a}, … (5 more values) …, 50, 52, 54, 56, 58")
        );
        assert!(
            lines[3].starts_with("Sensor B: 30 values, min 71, max 100: 100, 99,"),
            "{}",
            lines[3]
        );
        assert!(
            lines[3].ends_with("… (5 more values) …, 75, 74, 73, 72, 71"),
            "{}",
            lines[3]
        );
        assert_eq!(lines.len(), 4);
        // At exactly the limit nothing is summarised: every value, and the
        // one series with a format names it.
        let text = parse_chart_text_with(&chart(&title("Run"), &plot, ""), 60)
            .unwrap()
            .unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[1].starts_with("Sensor A, c0, c1, c2,"), "{}", lines[1]);
        assert_eq!(lines[2], "Number formats: Sensor A: 0");
    }

    /// Timestamp categories are all distinct: past the limit they are
    /// sampled too (EEvers, #4053 Q4); a value that is no number has no
    /// min/max but still its count and sample.
    #[test]
    fn distinct_labels_past_the_limit_are_sampled() {
        let labels = strings(40, |i| format!("2026-01-01 00:{i:02}"));
        let values = strings(40, |i| if i == 0 { "n/a".to_string() } else { format!("{i}") });
        let text = parse_chart_text_with(&chart("", &series("Logger", "General", &labels, &values), ""), 30)
            .unwrap()
            .unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines[0].starts_with("Categories (40): 2026-01-01 00:00, "),
            "{}",
            lines[0]
        );
        assert!(
            lines[0].contains("… (15 more labels) …, 2026-01-01 00:35"),
            "{}",
            lines[0]
        );
        assert!(
            lines[1].starts_with("Logger: 40 values, min 1, max 39: n/a, 1, 2,"),
            "{}",
            lines[1]
        );
        let only_text = strings(40, |i| format!("x{i}"));
        let text = parse_chart_text_with(&chart("", &series("Words", "General", &labels, &only_text), ""), 30)
            .unwrap()
            .unwrap();
        assert!(
            text.lines().nth(1).unwrap().starts_with("Words: 40 values: x0, x1"),
            "{text}"
        );
    }

    /// A series "name" that is really a range of thousands of cells is
    /// sampled too: it must not carry what the values no longer do.
    #[test]
    fn a_huge_series_name_is_sampled() {
        let labels = strings(40, |i| format!("t{i}"));
        let values = strings(40, |i| format!("{i}"));
        let name: String = (0..100)
            .map(|i| format!(r#"<c:pt idx="{i}"><c:v>n{i}</c:v></c:pt>"#))
            .collect();
        let plot = series("X", "General", &labels, &values).replace(r#"<c:pt idx="0"><c:v>X</c:v></c:pt>"#, &name);
        let text = parse_chart_text_with(&chart("", &plot, ""), 30).unwrap().unwrap();
        let line = text.lines().nth(1).unwrap();
        assert!(line.starts_with("n0, n1,"), "{line}");
        assert!(
            line.contains("… (75 more name parts) …, n95, n96, n97, n98, n99: 40 values"),
            "{line}"
        );
        assert!(text.len() < 1_000, "{}", text.len());
    }

    /// The default and the setter.
    #[test]
    fn the_limit_defaults_to_5000() {
        assert_eq!(DEFAULT_CHART_MAX_VALUES, 5_000);
        assert!(chart_max_values() > 0);
    }
}
