//! Flattening a filled form must *render* the value that was set.
//!
//! After `set_form_field_value` + `flatten_forms`, the document's stored
//! `/AP /N` is often an empty placeholder the form writer never refreshed, so
//! the flattener must regenerate the appearance from the new `/V` instead of
//! baking the blank placeholder into the page (which renders nothing). The
//! non-flatten counterpart (`/NeedAppearances`) is covered separately in
//! `form_fill_need_appearances.rs`.
//!
//! All tests hand-build their PDFs (single-field and a two-field CJK form) with
//! the relevant structure — merged field+widget, empty AP, a page `/Contents`
//! stream the overlay can append to — so no third-party fixture is needed.

use xberg_native_pdf::PdfDocument;
use xberg_native_pdf::editor::DocumentEditor;
use xberg_native_pdf::editor::form_fields::FormFieldValue;
use xberg_native_pdf::extractors::forms::FormExtractor;

/// Append `id 0 obj\n<body>\nendobj\n`, recording the object's byte offset.
fn obj(buf: &mut Vec<u8>, offsets: &mut [usize], id: usize, body: &str) {
    offsets[id] = buf.len();
    buf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    buf.extend_from_slice(body.as_bytes());
    buf.extend_from_slice(b"\nendobj\n");
}

/// Append a stream object (`<<dict>>\nstream\n<data>\nendstream`).
fn stream_obj(buf: &mut Vec<u8>, offsets: &mut [usize], id: usize, dict: &str, data: &[u8]) {
    offsets[id] = buf.len();
    buf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    buf.extend_from_slice(format!("<< {dict} /Length {} >>\nstream\n", data.len()).as_bytes());
    buf.extend_from_slice(data);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
}

/// A minimal one-field PDF: merged text field + widget with an empty `/AP /N`,
/// a real page `/Contents` stream, and a `/DR` Helvetica font. `field_name`
/// becomes the field's `/T`.
fn form_with_empty_ap(field_name: &str) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut off = vec![0usize; 8];

    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    obj(
        &mut buf,
        &mut off,
        1,
        "<< /Type /Catalog /Pages 2 0 R \
         /AcroForm << /Fields [4 0 R] /DA (/Helv 0 Tf 0 g) \
         /DR << /Font << /Helv 5 0 R >> >> >> >>",
    );
    obj(&mut buf, &mut off, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut buf,
        &mut off,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
         /Resources << /Font << /Helv 5 0 R >> >> /Contents 7 0 R /Annots [4 0 R] >>",
    );
    obj(
        &mut buf,
        &mut off,
        4,
        &format!(
            "<< /FT /Tx /T ({field_name}) /Type /Annot /Subtype /Widget \
             /Rect [72 700 400 720] /DA (/Helv 0 Tf 0 g) /AP << /N 6 0 R >> >>"
        ),
    );
    obj(
        &mut buf,
        &mut off,
        5,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    stream_obj(
        &mut buf,
        &mut off,
        6,
        "/Type /XObject /Subtype /Form /BBox [0 0 328 20]",
        b"/Tx BMC\nEMC\n",
    );
    stream_obj(&mut buf, &mut off, 7, "", b"q Q\n");

    let xref_off = buf.len();
    buf.extend_from_slice(b"xref\n0 8\n0000000000 65535 f \n");
    for id in 1..=7 {
        buf.extend_from_slice(format!("{:010} 00000 n \n", off[id]).as_bytes());
    }
    buf.extend_from_slice(b"trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n");
    buf.extend_from_slice(format!("{xref_off}\n%%EOF\n").as_bytes());
    buf
}

/// A minimal two-field fillable form (`full_name`, `city`) modelled on a
/// typical CJK form: each widget has an empty `/AP /N`, a `/DA` of
/// `(/He 8 Tf)`, and the AcroForm `/DR` defines `/He` as Helvetica. Hand-built
/// so the test owns the fixture (no third-party PDF).
fn cjk_form() -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut off = vec![0usize; 9];

    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    obj(
        &mut buf,
        &mut off,
        1,
        "<< /Type /Catalog /Pages 2 0 R \
         /AcroForm << /Fields [4 0 R 5 0 R] \
         /DR << /Font << /He 6 0 R >> >> >> >>",
    );
    obj(&mut buf, &mut off, 2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    obj(
        &mut buf,
        &mut off,
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
         /Resources << /Font << /He 6 0 R >> >> /Contents 8 0 R /Annots [4 0 R 5 0 R] >>",
    );
    obj(
        &mut buf,
        &mut off,
        4,
        "<< /FT /Tx /T (full_name) /Type /Annot /Subtype /Widget \
         /Rect [72 700 400 720] /DA (0 0 0 rg /He 8 Tf) /AP << /N 7 0 R >> >>",
    );
    obj(
        &mut buf,
        &mut off,
        5,
        "<< /FT /Tx /T (city) /Type /Annot /Subtype /Widget \
         /Rect [72 650 400 670] /DA (0 0 0 rg /He 8 Tf) /AP << /N 7 0 R >> >>",
    );
    obj(
        &mut buf,
        &mut off,
        6,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    stream_obj(
        &mut buf,
        &mut off,
        7,
        "/Type /XObject /Subtype /Form /BBox [0 0 328 20]",
        b"/Tx BMC\nEMC\n",
    );
    stream_obj(&mut buf, &mut off, 8, "", b"q Q\n");

    let xref_off = buf.len();
    buf.extend_from_slice(b"xref\n0 9\n0000000000 65535 f \n");
    for id in 1..=8 {
        buf.extend_from_slice(format!("{:010} 00000 n \n", off[id]).as_bytes());
    }
    buf.extend_from_slice(b"trailer\n<< /Size 9 /Root 1 0 R >>\nstartxref\n");
    buf.extend_from_slice(format!("{xref_off}\n%%EOF\n").as_bytes());
    buf
}

fn contains_ascii(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Fill `value` into the one field, flatten, and return the saved PDF bytes.
fn fill_and_flatten(field_name: &str, value: &str) -> Vec<u8> {
    let form = form_with_empty_ap(field_name);

    let doc0 = PdfDocument::from_bytes(form.clone()).unwrap();
    assert_eq!(
        FormExtractor::extract_fields(&doc0).unwrap().len(),
        1,
        "synthetic form should expose 1 field"
    );

    let mut ed = DocumentEditor::from_bytes(form).unwrap();
    ed.set_form_field_value(field_name, FormFieldValue::Text(value.into()))
        .unwrap();
    ed.flatten_forms().unwrap();
    ed.save_to_bytes().unwrap()
}

#[test]
fn flatten_renders_filled_latin_value() {
    let out = fill_and_flatten("full_name", "Hello");

    assert!(
        contains_ascii(&out, b"(Hello) Tj"),
        "flattened output must render the filled value via a text-show op"
    );

    assert!(
        !contains_ascii(&out, b"BMC\nEMC"),
        "the empty BMC..EMC placeholder must not be the baked appearance"
    );
}

#[test]
fn fixture_flatten_honors_da_font_and_size() {
    let bytes = cjk_form();
    let mut ed = DocumentEditor::from_bytes(bytes).unwrap();
    ed.set_form_field_value("full_name", FormFieldValue::Text("Hello".into()))
        .unwrap();
    ed.flatten_forms().unwrap();
    let out = ed.save_to_bytes().unwrap();

    assert!(
        contains_ascii(&out, b"/He 8 Tf"),
        "flattened appearance must honor the field /DA font + size (/He 8 Tf)"
    );
    assert!(
        !contains_ascii(&out, b"/Helv 10 Tf"),
        "must not emit the old hardcoded /Helv 10 Tf when /DA names /He 8"
    );
}

#[test]
fn flatten_provides_font_resource_for_appearance() {
    let out = fill_and_flatten("full_name", "Hello");

    assert!(
        contains_ascii(&out, b"/Font"),
        "flattened appearance XObject must carry a /Font resource"
    );
}

#[test]
fn flatten_renders_cjk_value() {
    let out = fill_and_flatten("city", "とうきょう");

    let has_text_op = contains_ascii(&out, b"Tj") || contains_ascii(&out, b"TJ");
    assert!(
        has_text_op,
        "flattened CJK appearance must contain a text-show operator"
    );
    assert!(
        !contains_ascii(&out, b"BMC\nEMC"),
        "the empty BMC..EMC placeholder must not be the baked appearance"
    );
}

/// Emoji needs a colour/emoji-capable embedded font; tracked as a separate
/// follow-up (font embedding). Ignored until that lands so it documents the
/// remaining gap without failing the suite.
#[test]
#[ignore = "follow-up: emoji needs an embedded colour/emoji font (font embedding)"]
fn flatten_renders_emoji_value() {
    let out = fill_and_flatten("full_name", "やまだたろう🍺");
    assert!(contains_ascii(&out, b"Tj") || contains_ascii(&out, b"TJ"));
}

#[test]
fn fixture_fill_and_flatten_renders_and_cleans_up() {
    let bytes = cjk_form();

    let doc = PdfDocument::from_bytes(bytes.clone()).unwrap();
    let names: Vec<String> = FormExtractor::extract_fields(&doc)
        .unwrap()
        .into_iter()
        .map(|f| f.full_name)
        .collect();
    assert!(
        names.iter().any(|n| n == "full_name") && names.iter().any(|n| n == "city"),
        "fixture should expose full_name + city; got {names:?}"
    );

    let mut ed = DocumentEditor::from_bytes(bytes).unwrap();
    ed.set_form_field_value("full_name", FormFieldValue::Text("やまだたろう".into()))
        .unwrap();
    ed.set_form_field_value("city", FormFieldValue::Text("とうきょう".into()))
        .unwrap();
    ed.flatten_forms().unwrap();
    let out = ed.save_to_bytes().unwrap();

    assert!(
        contains_ascii(&out, b"Tj") || contains_ascii(&out, b"TJ"),
        "flattened fixture must contain text-show operators for the filled values"
    );

    assert!(!contains_ascii(&out, b"/Annots"), "flatten must drop the page /Annots");

    assert!(PdfDocument::from_bytes(out).is_ok(), "flattened fixture must re-parse");
}

#[test]
fn fixture_non_flatten_roundtrips_cjk_value() {
    let bytes = cjk_form();
    let mut ed = DocumentEditor::from_bytes(bytes).unwrap();
    ed.set_form_field_value("full_name", FormFieldValue::Text("やまだたろう".into()))
        .unwrap();
    let out = ed.save_to_bytes().unwrap();

    let doc = PdfDocument::from_bytes(out.clone()).unwrap();
    let v = FormExtractor::extract_fields(&doc)
        .unwrap()
        .into_iter()
        .find(|f| f.full_name == "full_name")
        .map(|f| format!("{:?}", f.value))
        .unwrap_or_default();
    assert!(v.contains("やまだたろう"), "CJK value must round-trip; got {v}");
    assert!(
        contains_ascii(&out, b"NeedAppearances true"),
        "non-flatten fill must set /NeedAppearances true"
    );
}

#[cfg(feature = "cjk-form-fonts")]
#[test]
fn fixture_flatten_embeds_cjk_and_emoji_fonts() {
    let bytes = cjk_form();
    let mut ed = DocumentEditor::from_bytes(bytes).unwrap();
    ed.set_form_field_value("full_name", FormFieldValue::Text("やまだたろう🍺".into()))
        .unwrap();
    ed.set_form_field_value("city", FormFieldValue::Text("とうきょう".into()))
        .unwrap();
    ed.flatten_forms().unwrap();
    let out = ed.save_to_bytes().unwrap();

    assert!(
        contains_ascii(&out, b"FontFile2"),
        "must embed a TrueType fallback font program (FontFile2)"
    );
    assert!(contains_ascii(&out, b"Type0"), "must use a Type0 composite font");
    assert!(
        contains_ascii(&out, b"CIDFontType2"),
        "must use a CIDFontType2 descendant"
    );
    assert!(
        out.windows(4).any(|w| w == b"> Tj"),
        "must emit CID-keyed hex text for the embedded font"
    );
    assert!(PdfDocument::from_bytes(out).is_ok(), "embedded output must re-parse");
}

#[cfg(feature = "cjk-form-fonts")]
#[test]
fn synthetic_flatten_embeds_emoji() {
    let out = fill_and_flatten("full_name", "🍺");
    assert!(
        contains_ascii(&out, b"FontFile2") && contains_ascii(&out, b"CIDFontType2"),
        "emoji value must embed a glyph-capable font when flattened"
    );
}
