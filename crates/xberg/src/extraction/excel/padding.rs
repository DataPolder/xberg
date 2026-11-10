//! The padding a sheet carries past its data (X57, DataPolder #3899).
//!
//! A formula or a value dragged to the bottom of a sheet, or across to its
//! last column, leaves a block of cells that hold nothing but `0` or blanks.
//! The table cell limit (`security_limits.max_table_cells`) counts rows ×
//! widest row of every sheet, so such a block can push a workbook of a few
//! thousand real cells over the limit, and the whole document is refused. One
//! workbook seen in the field carried 55 rows of data and three columns of `0`
//! down to row 1,048,576: 27 million cells counted, about 1,400 real.
//!
//! Only padding at the END of a sheet is left out, before the sheet is read
//! into a grid and before anything is counted:
//!
//! - **Rows.** After the last row that holds a [`CellClass::Content`] cell, a
//!   trailing block of at least [`MIN_TRAILING_PADDING_ROWS`] rows is dropped.
//!   Every cell in it is `0` or blank by construction. A row in the middle of
//!   the sheet is never dropped, and neither is a short trailing run: a sheet
//!   that ends in a few all-zero rows is plausibly data.
//! - **Columns.** Columns after the last column that holds a content cell are
//!   dropped. A column with a header holds a content cell (its header), so it
//!   always stays, whatever its values; a column in the middle of the sheet
//!   always stays.
//!
//! A sheet without any content cell is left as it is.

use calamine::{Data, DataRef, Range};

/// The shortest trailing block of padding rows that is dropped. Padding left
/// by dragging a formula down runs to hundreds or thousands of rows; a real
/// sheet that ends in a few all-zero rows with no label or date is plausible
/// (a count table's last empty bins), and is kept.
pub(super) const MIN_TRAILING_PADDING_ROWS: u32 = 50;

/// What a cell contributes to a sheet's extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CellClass {
    /// No value, or text that is empty or whitespace only (NBSP included).
    Blank,
    /// The number zero.
    Zero,
    /// Anything else: text, a non-zero number, a date, a boolean, an error.
    Content,
}

fn class_of_text(text: &str) -> CellClass {
    if text.trim().is_empty() {
        CellClass::Blank
    } else {
        CellClass::Content
    }
}

pub(super) fn class_of_data(value: &Data) -> CellClass {
    match value {
        Data::Empty => CellClass::Blank,
        Data::String(s) => class_of_text(s),
        Data::Float(f) if *f == 0.0 => CellClass::Zero,
        Data::Int(0) => CellClass::Zero,
        _ => CellClass::Content,
    }
}

pub(super) fn class_of_data_ref(value: &DataRef<'_>) -> CellClass {
    match value {
        DataRef::Empty => CellClass::Blank,
        DataRef::String(s) => class_of_text(s),
        DataRef::SharedString(s) => class_of_text(s),
        DataRef::Float(f) if *f == 0.0 => CellClass::Zero,
        DataRef::Int(0) => CellClass::Zero,
        _ => CellClass::Content,
    }
}

/// A sheet's bounding box and the extent of its content, gathered cell by cell
/// (absolute positions).
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct SheetExtent {
    bounds: Option<(u32, u32, u32, u32)>,
    last_content_row: Option<u32>,
    last_content_col: Option<u32>,
}

impl SheetExtent {
    pub(super) fn observe(&mut self, row: u32, col: u32, class: CellClass) {
        let (row_min, row_max, col_min, col_max) = self.bounds.get_or_insert((row, row, col, col));
        *row_min = (*row_min).min(row);
        *row_max = (*row_max).max(row);
        *col_min = (*col_min).min(col);
        *col_max = (*col_max).max(col);
        if class == CellClass::Content {
            self.last_content_row = Some(self.last_content_row.map_or(row, |r| r.max(row)));
            self.last_content_col = Some(self.last_content_col.map_or(col, |c| c.max(col)));
        }
    }

    /// `(row_min, row_max, col_min, col_max)` of every cell seen; `None` for no cell.
    pub(super) fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        self.bounds
    }

    /// The last row and column to keep, when trailing padding is to be left
    /// out; `None` when nothing is (no padding, or no content to anchor on).
    pub(super) fn keep_until(&self) -> Option<(u32, u32)> {
        let (_, row_max, _, col_max) = self.bounds?;
        let last_row = self.last_content_row?;
        let last_col = self.last_content_col?;
        let end_row = if row_max - last_row >= MIN_TRAILING_PADDING_ROWS {
            last_row
        } else {
            row_max
        };
        let end = (end_row, last_col.min(col_max));
        (end != (row_max, col_max)).then_some(end)
    }

    /// The bounds once the trailing padding is left out.
    pub(super) fn kept_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (row_min, row_max, col_min, col_max) = self.bounds?;
        Some(match self.keep_until() {
            Some((end_row, end_col)) => (row_min, end_row, col_min, end_col),
            None => (row_min, row_max, col_min, col_max),
        })
    }
}

/// Leave a range's trailing padding out (the `.xls` / `.xlsb` / `.ods` path,
/// whose reader has no cell streamer: the range is already read). Returns the
/// range unchanged when there is nothing to leave out.
pub(super) fn trim_range(sheet_name: &str, range: Range<Data>) -> Range<Data> {
    let Some((start_row, start_col)) = range.start() else {
        return range;
    };
    let mut extent = SheetExtent::default();
    // `used_cells()`: a grid's empty fill is no cell, as in `worksheet_range`.
    for (row, col, value) in range.used_cells() {
        // Positions are relative to the range's start.
        extent.observe(start_row + row as u32, start_col + col as u32, class_of_data(value));
    }
    match extent.keep_until() {
        Some(end) => {
            log_trim(sheet_name, &extent, end);
            range.range((start_row, start_col), end)
        }
        None => range,
    }
}

pub(super) fn log_trim(sheet_name: &str, extent: &SheetExtent, end: (u32, u32)) {
    if let Some((_, row_max, _, col_max)) = extent.bounds() {
        tracing::debug!(
            sheet = sheet_name,
            padding_rows = row_max - end.0,
            padding_columns = col_max - end.1,
            "trailing 0/blank padding left out of the sheet (X57)"
        );
    }
}
