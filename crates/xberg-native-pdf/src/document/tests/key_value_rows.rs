fn key_value_span(text: &str, x: f32, y: f32) -> crate::layout::TextSpan {
    crate::layout::TextSpan {
        text: text.to_string(),
        bbox: crate::geometry::Rect::new(x, y, 90.0, 10.0),
        font_size: 11.0,
        ..crate::layout::TextSpan::default()
    }
}

#[test]
fn key_value_rows_keep_centered_labels_before_wrapped_values() {
    let mut spans = vec![
        key_value_span("Applicant:", 77.0, 600.0),
        key_value_span("Example Holdings", 231.0, 600.0),
        key_value_span("Respondent:", 77.0, 574.0),
        key_value_span("Alex Sample", 231.0, 574.0),
        key_value_span("12 Demo Street", 231.0, 555.0),
        key_value_span("Address:", 77.0, 548.0),
        key_value_span("X00Y000", 231.0, 541.0),
        key_value_span("Jordan Example", 231.0, 515.0),
        key_value_span("Panel:", 77.0, 508.0),
        key_value_span("Casey Placeholder", 231.0, 501.0),
        key_value_span("Venue:", 77.0, 475.0),
        key_value_span("Virtual", 231.0, 475.0),
    ];

    super::super::reorder_ltr_key_value_rows(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|span| span.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "Applicant:",
            "Example Holdings",
            "Respondent:",
            "Alex Sample",
            "Address:",
            "12 Demo Street",
            "X00Y000",
            "Panel:",
            "Jordan Example",
            "Casey Placeholder",
            "Venue:",
            "Virtual",
        ]
    );
}

#[test]
fn key_value_repair_leaves_aligned_two_column_rows_unchanged() {
    let mut spans = vec![
        key_value_span("Name:", 77.0, 600.0),
        key_value_span("Example", 231.0, 600.0),
        key_value_span("Role:", 77.0, 574.0),
        key_value_span("Reviewer", 231.0, 574.0),
        key_value_span("Venue:", 77.0, 548.0),
        key_value_span("Virtual", 231.0, 548.0),
        key_value_span("Date:", 77.0, 522.0),
        key_value_span("2030-01-15", 231.0, 522.0),
    ];
    let original: Vec<String> = spans.iter().map(|span| span.text.clone()).collect();

    super::super::reorder_ltr_key_value_rows(&mut spans);

    assert_eq!(spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>(), original);
}

#[test]
fn key_value_repair_leaves_rtl_rows_unchanged() {
    let mut spans = vec![
        key_value_span("שם:", 231.0, 600.0),
        key_value_span("ערך", 77.0, 607.0),
        key_value_span("נוסף", 77.0, 593.0),
        key_value_span("תפקיד:", 231.0, 574.0),
        key_value_span("בודק", 77.0, 574.0),
        key_value_span("מקום:", 231.0, 548.0),
        key_value_span("מרחוק", 77.0, 548.0),
        key_value_span("תאריך:", 231.0, 522.0),
        key_value_span("2030-01-15", 77.0, 522.0),
    ];
    let original: Vec<String> = spans.iter().map(|span| span.text.clone()).collect();

    super::super::reorder_ltr_key_value_rows(&mut spans);

    assert_eq!(spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>(), original);
}

#[test]
fn key_value_repair_ignores_a_larger_unrelated_right_column() {
    let mut spans = vec![
        key_value_span("Name:", 77.0, 600.0),
        key_value_span("Example", 231.0, 600.0),
        key_value_span("Role:", 77.0, 574.0),
        key_value_span("Reviewer", 231.0, 574.0),
        key_value_span("First address line", 231.0, 555.0),
        key_value_span("Address:", 77.0, 548.0),
        key_value_span("Second address line", 231.0, 541.0),
        key_value_span("Venue:", 77.0, 522.0),
        key_value_span("Virtual", 231.0, 522.0),
    ];
    for row in 0..8 {
        spans.push(key_value_span(
            &format!("unrelated-{row}"),
            500.0,
            600.0 - row as f32 * 10.0,
        ));
    }

    super::super::reorder_ltr_key_value_rows(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|span| span.text.as_str()).collect();
    assert_eq!(
        &texts[..9],
        [
            "Name:",
            "Example",
            "Role:",
            "Reviewer",
            "Address:",
            "First address line",
            "Second address line",
            "Venue:",
            "Virtual",
        ]
    );
    assert_eq!(texts[9], "unrelated-0");
}

#[test]
fn key_value_repair_handles_two_non_overlapping_blocks() {
    let mut spans = Vec::new();
    for (label_x, value_x, top, prefix) in [(50.0, 170.0, 600.0, "A"), (330.0, 450.0, 400.0, "B")] {
        spans.extend([
            key_value_span(&format!("{prefix}1:"), label_x, top),
            key_value_span(&format!("{prefix}v1"), value_x, top),
            key_value_span(&format!("{prefix}upper"), value_x, top - 19.0),
            key_value_span(&format!("{prefix}2:"), label_x, top - 26.0),
            key_value_span(&format!("{prefix}lower"), value_x, top - 33.0),
            key_value_span(&format!("{prefix}3:"), label_x, top - 52.0),
            key_value_span(&format!("{prefix}v3"), value_x, top - 52.0),
            key_value_span(&format!("{prefix}4:"), label_x, top - 78.0),
            key_value_span(&format!("{prefix}v4"), value_x, top - 78.0),
        ]);
    }

    super::super::reorder_ltr_key_value_rows(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|span| span.text.as_str()).collect();
    for prefix in ["A", "B"] {
        let label = texts
            .iter()
            .position(|text| *text == format!("{prefix}2:"))
            .expect("label");
        let upper = texts
            .iter()
            .position(|text| *text == format!("{prefix}upper"))
            .expect("upper");
        let lower = texts
            .iter()
            .position(|text| *text == format!("{prefix}lower"))
            .expect("lower");
        assert!(label < upper && upper < lower, "{prefix} block order: {texts:?}");
    }
}

/// X42: the first test's geometry with one label repeated is no form: the labels must name
/// different fields. ~keep
#[test]
fn key_value_repair_leaves_rows_with_one_repeated_label_unchanged_x42() {
    let mut spans = vec![
        key_value_span("vermeld in:", 77.0, 600.0),
        key_value_span("Example Holdings", 231.0, 600.0),
        key_value_span("vermeld in:", 77.0, 574.0),
        key_value_span("Alex Sample", 231.0, 574.0),
        key_value_span("12 Demo Street", 231.0, 555.0),
        key_value_span("vermeld in:", 77.0, 548.0),
        key_value_span("X00Y000", 231.0, 541.0),
        key_value_span("Jordan Example", 231.0, 515.0),
        key_value_span("vermeld in:", 77.0, 508.0),
        key_value_span("Casey Placeholder", 231.0, 501.0),
        key_value_span("vermeld in:", 77.0, 475.0),
        key_value_span("Virtual", 231.0, 475.0),
    ];
    let original: Vec<String> = spans.iter().map(|span| span.text.clone()).collect();

    super::super::reorder_ltr_key_value_rows(&mut spans);

    assert_eq!(spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>(), original);
}

/// X42: page 4 of a boiler's installation instructions -- four paragraphs each wrapped onto a
/// last line `vermeld in:` at the margin, with the section headings `1.1` to `1.4` beside
/// them. Every span of the page in the order the hierarchy pass hands them to the repair,
/// geometry verbatim, letters replaced by lorem ipsum of the same length (equal strings stay
/// equal). ~keep
#[test]
fn installation_instructions_with_a_repeated_wrap_line_are_not_a_form_x42() {
    #[rustfmt::skip]
    let page: &[(f32, f32, f32, f32, &str)] = &[
        (87.98, 784.80, 6.40, 14.04, "1"),
        (94.34, 784.80, 3.90, 14.04, " "),
        (105.98, 784.80, 142.23, 14.04, "ORSITAMETCONSECTETUR"),
        (248.33, 784.80, 30.02, 14.04, "LIQU"),
        (278.21, 784.80, 3.20, 14.04, " "),
        (45.36, 769.80, 291.68, 9.48, "Do loremagna Aliqualo Remipsumdo LO rsitametc onse ctetur adipiscingelitsed doei "),
        (45.36, 758.88, 223.29, 9.48, "umdolo rs itamet consectetur adip isc inge (litsed) doeiusm odt em "),
        (45.36, 747.94, 279.54, 9.48, "cteturadipiscingelitsedd oe -iusmodtempo, rin cid idun tutlaboreetdol oremagn aal "),
        (45.36, 737.02, 278.73, 9.48, "gnaaliqualo rem ip Sumdolor Sitamet Cons Ecte Turadipis cingelitsedd oe iu "),
        (45.36, 726.10, 128.66, 9.48, "dolorsita metconsectet uradipiscin. "),
        (45.36, 712.30, 322.38, 9.48, "Sci ngelitse dd oeiu smodtem pori ncididu ntut laboreet (doloremag naaliqua) lor emipsumdolo "),
        (45.36, 701.38, 257.47, 9.48, "mdolorsitame, tconsectetur ad ipiscingeli tseddoeiu, sm odtemp ori ncididun t"),
        (303.05, 701.38, 63.09, 9.48, "t labore, etdolo rem "),
        (45.36, 690.46, 331.52, 9.48, "gelitsed do eiusmodtem pori nci diduntu tla bor eetdolor emag naa liqualo rem ipsumdolorsitame tc "),
        (45.36, 679.54, 106.86, 9.48, "reet dol oremagnaal iq ualorem."),
        (152.42, 679.54, 2.16, 9.48, " "),
        (45.36, 665.62, 260.81, 9.48, "Psum do lorsitametcon secteturadi pisc in gelitseddoeiu smodtempor incidid. "),
        (45.36, 651.70, 2.16, 9.48, " "),
        (87.98, 630.58, 13.72, 12.00, "1.1"),
        (101.66, 630.58, 3.34, 12.00, " "),
        (105.98, 630.58, 49.78, 12.00, "Tametcon "),
        (45.36, 617.02, 297.00, 9.48, "Do lorsit ametconsect etur adipisc ing el itseddoe (iusmodtempo-) rincididuntut, labor "),
        (45.36, 606.22, 40.35, 9.48, "tconsec te: "),
        (45.36, 592.30, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 592.30, 2.24, 8.04, " "),
        (63.36, 592.30, 98.78, 9.48, "Amet consecteturadipiscingeli. "),
        (45.36, 581.38, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 581.38, 2.24, 8.04, " "),
        (63.36, 581.38, 144.41, 9.48, "TEM 1087: Porincidid unt utlaboreetdo. "),
        (45.36, 570.43, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 570.43, 2.24, 8.04, " "),
        (63.36, 570.43, 202.61, 9.48, "UTL 3215: Aboreetdolorema gn aaliqual or emipsumdolor. "),
        (45.36, 559.51, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 559.51, 2.24, 8.04, " "),
        (63.36, 559.51, 58.22, 9.48, "Olo rsitametcon. "),
        (45.36, 548.59, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 548.59, 2.24, 8.04, " "),
        (63.36, 548.59, 235.73, 9.48, "Lorsitametco nsecteturadip isc ingelits, eddoeiusm od temporincidid. "),
        (45.36, 537.79, 2.16, 9.48, " "),
        (87.98, 516.67, 13.72, 12.00, "1.2"),
        (101.66, 516.67, 3.34, 12.00, " "),
        (105.98, 516.67, 65.14, 12.00, "DO-eiusmodtemp "),
        (45.36, 503.11, 297.00, 9.48, "Do lorsit ametconsect etur adipisc ing el itseddoe (iusmodtempo-) rincididuntut, labor "),
        (45.36, 492.19, 40.35, 9.48, "tconsec te: "),
        (45.36, 478.27, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 478.27, 2.24, 8.04, " "),
        (63.36, 478.27, 167.57, 9.48, "ORS 3028: Itametconsectetu radi PI -scingelitsed. "),
        (45.36, 467.35, 2.16, 9.48, " "),
        (87.98, 446.23, 13.72, 12.00, "1.3"),
        (101.66, 446.23, 3.34, 12.00, " "),
        (105.98, 446.23, 66.70, 12.00, "Doeiusmodtempo "),
        (45.36, 432.67, 297.00, 9.48, "Do lorsit ametconsect etur adipisc ing el itseddoe (iusmodtempo-) rincididuntut, labor "),
        (45.36, 421.75, 40.35, 9.48, "tconsec te: "),
        (45.36, 407.83, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 407.83, 2.24, 8.04, " "),
        (63.36, 407.83, 63.38, 9.48, "ORE 1078 (1987). "),
        (45.36, 397.01, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 397.01, 2.24, 8.04, " "),
        (63.36, 397.01, 127.82, 9.48, "AAL 1088: Iqualoremip su MDO 1087. "),
        (45.36, 386.09, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 386.09, 2.24, 8.04, " "),
        (63.36, 386.09, 126.86, 9.48, "LIT 3378: Seddoeiusmo dte MPO 1078. "),
        (45.36, 375.17, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 375.17, 2.24, 8.04, " "),
        (63.36, 375.17, 205.73, 9.48, "MPO 2920: Rinci didu ntutlaboreetdo loremagnaaliqualoremips. "),
        (45.36, 364.25, 2.16, 9.48, " "),
        (87.98, 343.13, 13.72, 12.00, "1.4"),
        (101.66, 343.13, 3.34, 12.00, " "),
        (105.98, 343.13, 103.45, 12.00, "Iqualoremip sumdolorsit "),
        (45.36, 329.57, 297.00, 9.48, "Do lorsit ametconsect etur adipisc ing el itseddoe (iusmodtempo-) rincididuntut, labor "),
        (45.36, 318.65, 40.35, 9.48, "tconsec te: "),
        (45.36, 304.73, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 304.73, 2.24, 8.04, " "),
        (63.36, 304.73, 39.38, 9.48, "SEC 1010. "),
        (45.36, 293.93, 2.16, 9.48, " "),
        (87.98, 272.81, 13.72, 12.00, "1.5"),
        (101.66, 272.81, 3.34, 12.00, " "),
        (105.98, 272.81, 99.01, 12.00, "Loremipsumdolorsitame "),
        (45.36, 259.25, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 259.25, 2.24, 8.04, " "),
        (63.36, 259.25, 275.95, 9.48, "IUS 1006: Modtempo rincididuntut labo reetdolore magnaaliqual (or emipsumdol ors "),
        (63.36, 248.33, 101.30, 9.48, "lab oreetdol oremagnaa liqual). "),
        (45.36, 237.41, 2.16, 9.48, " "),
        (87.98, 216.26, 13.72, 12.00, "1.6"),
        (101.66, 216.26, 3.34, 12.00, " "),
        (105.98, 216.26, 149.89, 12.00, "Iusmodtempori nc ididuntutlab "),
        (45.36, 202.70, 217.97, 9.48, "Ec teturadipisci ng elitseddoeiusmodtempori ncid iduntut lab: "),
        (45.36, 188.78, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 188.78, 2.24, 8.04, " "),
        (63.36, 188.78, 278.31, 9.48, "RIN 1078 (1987) cididu ntut: Laboreet doloremagn aa liqualoremipsumdol, orsitam etc "),
        (63.36, 177.86, 241.25, 9.48, "cteturadip isc ing elitseddoeiusmodte mporinc idi 17% (duntutlabore). "),
        (45.36, 167.06, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 167.06, 2.24, 8.04, " "),
        (63.36, 167.06, 210.26, 9.48, "IPS 2757: Umdolor sitametconsectetu ra dipisc ing elit sed "),
        (63.36, 156.14, 80.42, 9.48, "eturadipiscingelitsedd. "),
        (45.36, 145.22, 3.70, 8.04, "\u{f0b7}"),
        (48.96, 145.22, 2.24, 8.04, " "),
        (63.36, 145.22, 204.17, 9.48, " OEI 3378, usm. 8.1.2: Odtemporincid id untut laboreetdolo. "),
        (45.36, 17.88, 115.41, 9.96, "Ingelits Eddoeiusmo DT"),
        (160.82, 17.88, 5.65, 9.96, "  "),
        (544.18, 17.88, 4.54, 9.96, "4"),
        (548.74, 17.88, 2.37, 9.48, " "),
    ];
    let mut spans: Vec<crate::layout::TextSpan> = page
        .iter()
        .map(|&(x, y, width, size, text)| crate::layout::TextSpan {
            text: text.to_string(),
            bbox: crate::geometry::Rect::new(x, y, width, size),
            font_size: size,
            ..crate::layout::TextSpan::default()
        })
        .collect();
    let original: Vec<String> = spans.iter().map(|span| span.text.clone()).collect();

    super::super::reorder_ltr_key_value_rows(&mut spans);

    assert_eq!(spans.iter().map(|span| span.text.clone()).collect::<Vec<_>>(), original);
}
