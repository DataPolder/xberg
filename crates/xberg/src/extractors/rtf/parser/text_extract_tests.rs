use super::extract_text_from_rtf;

/// #86: `\shptxt` (drawing-object/text-box text) is a plain destination,
/// but real producers nest it inside an ignorable `{\*\shp{\*\shpinst
/// ...}}` ancestor. Before the fix, the outer ignorable-and-unrecognized
/// destination set `skip_depth` for the whole subtree, and the nested
/// `\shptxt` group -- despite being recognized -- had no way to escape
/// that already-active skip, so its text was dropped.
#[test]
fn test_shptxt_survives_nested_ignorable_ancestor() {
    let rtf = r"{\rtf1\ansi
{\*\shp{\*\shpinst{\sp{\sn shapeType}{\sv202}}{\shptxt Text box content}}}
Body text after the shape.\par
}";
    let (text, _tables, _images, _para_metas, _fmt) = extract_text_from_rtf(rtf, false);

    assert!(
        text.contains("Text box content"),
        "text-box content should be extracted, got: {text:?}"
    );
    assert!(
        text.contains("Body text after the shape."),
        "ordinary body text should be unaffected, got: {text:?}"
    );
}

/// #86: `\annotation` (Word comment text) was previously treated as an
/// unrecognized ignorable destination and skipped whole. `\atnid` carries
/// the comment's numeric id and should label the extracted comment.
#[test]
fn test_annotation_and_atnid_are_extracted_as_labeled_comment() {
    let rtf = r"{\rtf1\ansi
Body text.\par
{\annotation{\*\atnid7}Reviewer comment here}
}";
    let (text, _tables, _images, _para_metas, _fmt) = extract_text_from_rtf(rtf, false);

    assert!(
        text.contains("[Comment 7]: Reviewer comment here"),
        "comment should be extracted and labeled with its atnid, got: {text:?}"
    );
    assert!(
        text.contains("Body text."),
        "ordinary body text should be unaffected, got: {text:?}"
    );
}

/// A comment with no `\atnid` falls back to a running 1-based counter
/// rather than being dropped or mislabeled.
#[test]
fn test_annotation_without_atnid_falls_back_to_counter_label() {
    let rtf = r"{\rtf1\ansi
{\annotation First comment}
{\annotation Second comment}
}";
    let (text, _tables, _images, _para_metas, _fmt) = extract_text_from_rtf(rtf, false);

    assert!(text.contains("[Comment 1]: First comment"), "got: {text:?}");
    assert!(text.contains("[Comment 2]: Second comment"), "got: {text:?}");
}
