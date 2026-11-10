//! X57 (DataPolder #3899): trailing 0/blank padding is left out of a sheet
//! before it is read into a grid and before any cell is counted.

use super::*;

/// Column letters for a 0-based column index (`0` → `A`, `26` → `AA`).
fn col_letters(mut col: u32) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// One cell of a test sheet: `""` writes no cell, `"0"` a numeric zero, a
/// number a number, `"~"` a whitespace-only string, `"nbsp"` an NBSP-only
/// string, anything else an inline string.
fn cell_xml(row: u32, col: u32, spec: &str) -> String {
    let r = format!("{}{}", col_letters(col), row + 1);
    match spec {
        "" => String::new(),
        "~" => format!(r#"<c r="{r}" t="inlineStr"><is><t xml:space="preserve">   </t></is></c>"#),
        "nbsp" => format!("<c r=\"{r}\" t=\"inlineStr\"><is><t>\u{a0}</t></is></c>"),
        s if s.parse::<f64>().is_ok() => format!(r#"<c r="{r}"><v>{s}</v></c>"#),
        s => format!(r#"<c r="{r}" t="inlineStr"><is><t>{s}</t></is></c>"#),
    }
}

fn sheet_xml(rows: &[Vec<&str>]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
    );
    for (r, cells) in rows.iter().enumerate() {
        xml.push_str(&format!(r#"<row r="{}">"#, r + 1));
        for (c, spec) in cells.iter().enumerate() {
            xml.push_str(&cell_xml(r as u32, c as u32, spec));
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    xml
}

/// Read a one-sheet xlsx built from `rows` through the production path.
fn read(rows: &[Vec<&str>]) -> ExcelSheet {
    let bytes = make_xlsx_with_worksheet(&sheet_xml(rows));
    let mut workbook = calamine::Xlsx::new(Cursor::new(bytes.as_slice())).unwrap();
    let mut warnings = Vec::new();
    process_xlsx_sheet_safe(&mut workbook, "Sheet1", &mut warnings).unwrap()
}

/// Rows × widest row of the sheet's grid: what `max_table_cells` counts.
fn counted(sheet: &ExcelSheet) -> usize {
    let cells = sheet.table_cells.as_ref().unwrap();
    cells.len() * cells.iter().map(Vec::len).max().unwrap_or(0)
}

fn data() -> Vec<Vec<&'static str>> {
    vec![
        vec!["Date", "Sensor 1", "Sensor 2"],
        vec!["2026-01-01", "1.5", "2.5"],
        vec!["2026-01-02", "3.5", "4.5"],
    ]
}

/// A column of `0` dragged to the bottom: the trailing block goes.
#[test]
fn a_trailing_block_of_zero_rows_is_left_out() {
    let mut rows = data();
    rows.extend((0..200).map(|_| vec!["", "", "0"]));
    let sheet = read(&rows);
    assert_eq!(sheet.table_cells.as_ref().unwrap().len(), 3);
    assert_eq!(counted(&sheet), 9);
    assert!(!sheet.markdown.contains("| 0 |"), "{}", sheet.markdown);
}

/// Trimming must not bring back what `worksheet_range` never shows: a
/// value-less styled cell (`<c r=.. s=..>`) inside the kept extent stays out of
/// the grid. Found in the corpus A/B: headerless trailing columns of 0 were
/// cut, and the rows of styled empty cells came back as blank table rows.
#[test]
fn styled_empty_cells_stay_out_when_padding_is_trimmed() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c><c r="B1" t="inlineStr"><is><t>Cost</t></is></c><c r="D1" s="1"/></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>Paper</t></is></c><c r="B2"><v>12</v></c><c r="D2"><v>0</v></c></row>
<row r="3"><c r="A3" t="inlineStr"><is><t>Ink</t></is></c><c r="B3"><v>30</v></c><c r="D3"><v>0</v></c></row>
<row r="4"><c r="A4" s="1"/><c r="B4" s="1"/></row>
<row r="5"><c r="A5" t="inlineStr"><is><t>Total</t></is></c><c r="B5"><v>42</v></c><c r="D5"><v>0</v></c></row>
<row r="6"><c r="A6" s="1"/><c r="B6" s="1"/></row>
<row r="7"><c r="A7" s="1"/><c r="B7" s="1"/></row>
</sheetData></worksheet>"#;
    let bytes = make_xlsx_with_worksheet(xml);
    let mut workbook = calamine::Xlsx::new(Cursor::new(bytes.as_slice())).unwrap();
    let mut warnings = Vec::new();
    let sheet = process_xlsx_sheet_safe(&mut workbook, "Sheet1", &mut warnings).unwrap();
    // Column D (no header, only 0) is left out; rows 6-7 (styled, no value)
    // were never part of the grid; row 4 sits between data rows, as before.
    assert_eq!(
        sheet.table_cells.unwrap(),
        vec![
            vec!["Item".to_string(), "Cost".to_string()],
            vec!["Paper".to_string(), "12".to_string()],
            vec!["Ink".to_string(), "30".to_string()],
            vec![String::new(), String::new()],
            vec!["Total".to_string(), "42".to_string()],
        ]
    );
}

/// The boundary: 49 trailing padding rows stay, 50 go.
#[test]
fn a_short_trailing_run_stays_and_the_boundary_is_fifty() {
    for (padding, kept) in [(49usize, 3 + 49), (50, 3)] {
        let mut rows = data();
        rows.extend((0..padding).map(|_| vec!["", "0", "0"]));
        assert_eq!(read(&rows).table_cells.unwrap().len(), kept, "{padding} padding rows");
    }
}

/// Whitespace-only and NBSP-only text is blank; a block of it goes.
#[test]
fn whitespace_and_nbsp_rows_are_padding() {
    let mut rows = data();
    rows.extend((0..60).map(|i| vec![if i % 2 == 0 { "~" } else { "nbsp" }, "", ""]));
    assert_eq!(read(&rows).table_cells.unwrap().len(), 3);
}

/// A row of zeros WITH its date is data, also at the end of the sheet; a
/// zero row in the middle is never touched.
#[test]
fn dated_zero_rows_and_a_mid_sheet_zero_row_stay() {
    let mut rows = data();
    rows.push(vec!["2026-01-03", "0", "0"]);
    rows.push(vec!["", "0", "0"]);
    rows.push(vec!["2026-01-05", "6.5", "7.5"]);
    rows.extend((0..60).map(|i| {
        if i == 59 {
            vec!["2026-03-01", "0", "0"]
        } else {
            vec!["", "", ""]
        }
    }));
    let sheet = read(&rows);
    // The last row carries a date: content, so nothing after the data is padding.
    assert_eq!(sheet.table_cells.as_ref().unwrap().len(), rows.len());
    let cells = sheet.table_cells.unwrap();
    assert_eq!(cells.last().unwrap()[0], "2026-03-01");
    assert!(cells.iter().any(|r| r[0] == "2026-01-03"));
    assert!(
        cells.iter().any(|r| r[0].is_empty() && r[1] == "0"),
        "the mid-sheet zero row stays"
    );
}

/// A trailing column without a header holding only `0` goes; a column with a
/// header stays whatever its values; an empty column in the middle stays.
#[test]
fn only_trailing_headerless_padding_columns_go() {
    let rows = vec![
        vec!["Date", "", "Sensor", "Spare", "", ""],
        vec!["2026-01-01", "", "1.5", "0", "0", "0"],
        vec!["2026-01-02", "", "2.5", "0", "0", "0"],
    ];
    let sheet = read(&rows);
    let cells = sheet.table_cells.unwrap();
    assert_eq!(cells[0], vec!["Date", "", "Sensor", "Spare"]);
    assert!(cells.iter().all(|r| r.len() == 4));
}

/// Dragged to the last column: a headerless row of `0` across to column XFD.
#[test]
fn a_row_dragged_to_the_last_column_is_left_out() {
    let mut rows = data();
    let mut dragged = vec!["", "", ""];
    dragged.extend(std::iter::repeat_n("0", 16_384 - 3));
    rows.insert(2, dragged);
    let sheet = read(&rows);
    assert_eq!(sheet.col_count, 3);
    assert_eq!(counted(&sheet), 4 * 3);
}

/// A sheet without a single content cell is left as it is.
#[test]
fn a_sheet_of_only_padding_is_left_as_it_is() {
    let rows: Vec<Vec<&str>> = (0..80).map(|_| vec!["0", "~"]).collect();
    assert_eq!(read(&rows).table_cells.unwrap().len(), 80);
}

/// The `.xls` / `.xlsb` / `.ods` path trims the range it already read, by the
/// same rule.
#[test]
fn a_read_range_is_trimmed_by_the_same_rule() {
    let mut range: Range<Data> = Range::new((0, 0), (99, 5));
    range.set_value((0, 0), Data::String("Date".into()));
    range.set_value((0, 2), Data::String("Spare".into()));
    range.set_value((1, 0), Data::String("2026-01-01".into()));
    range.set_value((1, 1), Data::Float(1.5));
    for row in 1..100 {
        range.set_value((row, 2), Data::Float(0.0));
        range.set_value((row, 5), Data::Int(0));
    }
    let trimmed = crate::extraction::excel::padding::trim_range("Sheet1", range);
    assert_eq!(trimmed.start(), Some((0, 0)));
    assert_eq!(trimmed.end(), Some((1, 2)));

    // Nothing to leave out: the range comes back as it was.
    let mut kept: Range<Data> = Range::new((0, 0), (1, 1));
    kept.set_value((0, 0), Data::String("a".into()));
    kept.set_value((0, 1), Data::String("b".into()));
    kept.set_value((1, 1), Data::Float(0.0));
    assert_eq!(
        crate::extraction::excel::padding::trim_range("Sheet1", kept).end(),
        Some((1, 1))
    );
}

/// What counts as padding: blank text and zero, never a date, a boolean or
/// non-blank text.
#[test]
fn the_cell_classes() {
    use crate::extraction::excel::padding::{CellClass, class_of_data};
    assert_eq!(class_of_data(&Data::Empty), CellClass::Blank);
    assert_eq!(class_of_data(&Data::String(" \u{a0}\t".into())), CellClass::Blank);
    assert_eq!(class_of_data(&Data::Float(0.0)), CellClass::Zero);
    assert_eq!(class_of_data(&Data::Float(-0.0)), CellClass::Zero);
    assert_eq!(class_of_data(&Data::Int(0)), CellClass::Zero);
    assert_eq!(class_of_data(&Data::Float(0.001)), CellClass::Content);
    assert_eq!(class_of_data(&Data::String("0".into())), CellClass::Content);
    assert_eq!(class_of_data(&Data::Bool(false)), CellClass::Content);
}
