/// Multi-row-spanning label cell (test item name vertically centered
/// across N data rows) must be placed at the top of its row block in
/// reading-order output, not interleaved mid-group by Y.
///
/// Simulates a simplified 2-column table:
/// - Column A (sparse, "labels"): 2 labels, each centered in its
///   block of 6 data rows.
/// - Column B (dense, "data"): 12 data rows.
///
/// Expected sort: Label1, d1..d6, Label2, d7..d12.
#[test]
fn test_rowspan_label_promoted_to_top_of_block() {
    use crate::layout::TextSpan;

    fn mk(text: &str, x: f32, y: f32, w: f32) -> TextSpan {
        TextSpan {
            provenance: None,
            text_rise: 0.0,
            artifact_type: None,
            text: text.to_string(),
            bbox: crate::geometry::Rect::new(x, y, w, 10.0),
            font_size: 12.0,
            font_name: "Arial".into(),
            font_weight: crate::layout::FontWeight::Normal,
            is_italic: false,
            is_monospace: false,
            color: crate::layout::Color::black(),
            mcid: None,
            mcid_scope: None,
            sequence: 0,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            primary_detected: false,
            char_widths: vec![],
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: 0.0,
            wmode: 0,
            rtl_draw_logical: false,
            mirrored: false,
            page_rotation_applied: 0,
        }
    }

    // Data rows at x=200, y=100..30 step -10 (12 rows).
    // Label1 at x=50, y=75 (middle of rows 100..60).
    // Label2 at x=50, y=45 (middle of rows 50..30... but actually 50..30 is 3 values,
    //   and label2 should be centered in rows 50..30 → y=40 but we choose 45 to be clearly in 2nd block).
    // Target split: Label1 owns rows 100,90,80,70,60,50; Label2 owns 40,30,20,10.
    // Both labels' Y (75 and 45) sit between their block rows. ~keep
    let mut spans = vec![mk("L1", 50.0, 75.0, 40.0), mk("L2", 50.0, 45.0, 40.0)];
    for i in 0..12 {
        let y = 100.0 - (i as f32) * 10.0;
        spans.push(mk(&format!("d{:02}", i), 200.0, y, 20.0));
    }

    super::super::PdfDocument::reorder_rowspan_labels(&mut spans);

    let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
    let pos_l1 = texts.iter().position(|t| *t == "L1").expect("L1 present");
    let pos_l2 = texts.iter().position(|t| *t == "L2").expect("L2 present");
    assert!(
        pos_l1 < pos_l2,
        "L1 should precede L2 in reading order, got {:?}",
        texts
    );
    // L1 must come before ALL data rows that belong to L1's block.
    // With distance-based partitioning, L1 owns rows closer to y=75 than y=45:
    //   100,90,80,70,60 are closer to 75. 50 is equidistant (tie → L1).
    //   Expect L1 at index 0 and L2 somewhere after L1's block. ~keep
    assert_eq!(texts[0], "L1", "L1 must be first, got: {:?}", &texts[..5]);
    assert!(
        pos_l2 > pos_l1 + 3,
        "L2 must come after several data rows of L1's block, got {:?}",
        texts
    );
}

/// X44: build a `Table` from text rows (an empty string is an empty cell).
fn x44_table(rows: &[&[&str]]) -> crate::structure::Table {
    let mut table = crate::structure::Table::new();
    table.col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    for row in rows {
        let mut table_row = crate::structure::TableRow::new(false);
        for text in *row {
            table_row
                .cells
                .push(crate::structure::TableCell::new((*text).to_string(), false));
        }
        table.rows.push(table_row);
    }
    table
}

/// X44: the kiwa EN 50518 mapping matrix (p50), a ruled nine-column grid whose
/// cells wrap; every wrapped tail is its own row and opens lowercase. Kept.
#[test]
fn x44_ruled_grid_with_wrapped_cells_is_not_prose() {
    let table = x44_table(&[
        &[
            "European standard",
            "EN 50518",
            "EN-IEC 62676-4:",
            "IEC 60839-11-2:",
            "K21023",
            "EN 50136-1/A1",
            "CLC/TS 50134-7:",
            "ISO/IEC 27039; 2015",
            "TS54-14: 2004",
        ],
        &[
            "Name of the standard",
            "Monitoring and alarm",
            "Video",
            "Alarm &",
            "Mobile Security –",
            "Alarm",
            "Alarm systems -",
            "Information technology",
            "Fire Alarms Systems",
        ],
        &[
            "",
            "receiving centre",
            "surveillance",
            "electronic",
            "Security of mobile",
            "Transmission",
            "Social alarm",
            "Security techniques -",
            "(FAS)",
        ],
        &[
            "",
            "",
            "systems for use",
            "security",
            "objects and",
            "Service Provider",
            "systems - Part 7:",
            "Selection, deployment",
            "",
        ],
        &[
            "",
            "",
            "in security",
            "systems - Part",
            "persons",
            "",
            "Application",
            "and operations of",
            "",
        ],
        &[
            "",
            "",
            "applications -",
            "11-2: Electronic",
            "",
            "",
            "guidelines",
            "intrusion detection",
            "",
        ],
        &["", "", "Part 4:", "access control", "", "", "", "systems (IDPS)", ""],
    ]);
    assert!(
        super::super::looks_like_prose_table(&table),
        "the old verdict counts the wraps"
    );
    assert!(
        !super::super::looks_like_prose_ruled_table(&table),
        "wrapped cells under a full header are a table"
    );
}

/// X44: kiwa p52 shape — four columns, a full header, lowercase wraps under filled cells.
#[test]
fn x44_four_column_grid_with_wraps_is_kept() {
    let table = x44_table(&[
        &["Omschrijving", "Omschrijving 2", "Reactietijd", "Extra"],
        &[
            "Stroomstoring",
            "Herstel stroomstoring",
            "12 uur als er nog een",
            "180s bij volledige",
        ],
        &["", "", "backup stroom is", "uitval"],
        &[
            "Communicatieuitval",
            "Herstel",
            "Eigen afspraken met klant",
            "dubbele uitval is",
        ],
        &["", "communicatieuitval", "", "prio 2"],
        &["Sabotage detector", "Herstel sabotage", "prio 2", ""],
    ]);
    assert!(!super::super::looks_like_prose_ruled_table(&table));
}

/// X44: prose boxed by a rule grid continues sideways, mid-clause, into the next
/// column; those fragments still count, so the grid is still refused.
#[test]
fn x44_prose_split_across_columns_is_still_refused() {
    let table = x44_table(&[
        &["Public divided over", "which level of", "government is"],
        &["primarily responsible", "for policies to", "limit the spread"],
        &["Which should be", "mainly responsible", "for developing"],
        &["and executing the", "policies to limit", "the spread of"],
        &["the coronavirus in", "the country and", "in the state"],
    ]);
    assert!(super::super::looks_like_prose_ruled_table(&table));
}

/// X44: a header row with an empty cell keeps the old verdict, wraps and all
/// (figure panels and flowed text open on a sparse first row).
#[test]
fn x44_header_with_an_empty_cell_keeps_the_old_verdict() {
    let table = x44_table(&[
        &["LLaVA-Instruct-150K", "", ""],
        &["", "In-context:", "Query:"],
        &["", "instruction: what type", "instruction: what is the"],
        &["", "device can be seen in", "lights in the image?"],
        &["", "response: there is a", "response: all the"],
        &["", "laptop on the desk", "traffic lights are"],
    ]);
    assert_eq!(
        super::super::looks_like_prose_ruled_table(&table),
        super::super::looks_like_prose_table(&table)
    );
    assert!(super::super::looks_like_prose_ruled_table(&table));
}
