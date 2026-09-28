//! Format dispatch and read entry points (file/bytes -> parsed workbook) for the
//! Excel/ODS extraction module, split out of `excel.rs` to keep that file under the
//! line-count limit.

use super::*;

/// Reject a ZIP-backed spreadsheet (XLSX/XLSM/XLTM/XLAM/XLSB/ODS) whose ZIP central
/// directory declares more entries than the caller's configured
/// `SecurityLimits::max_files_in_archive`, or whose declared aggregate uncompressed
/// size or compression ratio exceeds `SecurityLimits::max_archive_size` /
/// `max_compression_ratio` (`ZipBombValidator`).
///
/// `calamine::Xlsx`/`Xlsb`/`Ods` own the `zip::ZipArchive` internally end-to-end
/// (`Reader::new` opens it and immediately reads workbook/style/shared-string parts
/// before returning) and never expose the archive, an entry count, or per-entry
/// sizes, so none of these limits can be enforced from inside calamine's read path.
/// This reads the archive's central directory once, ahead of the calamine open,
/// purely to enforce them — parsing the central directory does not decompress any
/// entry, so this is not "after the bomb has already been expanded." Errors out
/// rather than truncating, matching the DOCX/PPTX top-level containers
/// (`extraction::docx::parser::validate_archive_security`,
/// `extraction::pptx::container::check_entry_count` +
/// `extractors::security::ZipBombValidator`): a workbook depends on specific named
/// parts (`xl/workbook.xml`, `xl/_rels/workbook.xml.rels`, per-sheet XML) that cannot
/// survive an arbitrary truncation of the ZIP's central directory.
///
/// The entry-count check runs first and keeps its own specific error message
/// (existing tests assert on it); `ZipBombValidator::validate` also re-checks entry
/// count using the same `limits.max_files_in_archive`, which is a no-op here since
/// the explicit check above already returned on that condition, but keeps this
/// function equivalent to calling the validator directly.
///
/// Skips ZIP validation only when a legacy `.xls`/`.xla` input has the OLE compound-file
/// signature. An embedded ZIP can otherwise make `ZipArchive` accept the outer OLE bytes,
/// then fail while resolving a member against the wrong container (#1938). Modern spreadsheet
/// extensions retain ZIP validation even when their bytes begin with an OLE signature.
#[cfg(feature = "excel")]
fn validate_zip_container<R: Read + Seek>(mut reader: R, file_extension: &str, limits: &SecurityLimits) -> Result<()> {
    const OLE_SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

    let initial_position = reader.stream_position()?;
    let mut signature = [0u8; OLE_SIGNATURE.len()];
    let has_ole_signature = reader.read_exact(&mut signature).is_ok() && signature == OLE_SIGNATURE;
    reader.seek(std::io::SeekFrom::Start(initial_position))?;

    let extension = file_extension.to_ascii_lowercase();
    if matches!(extension.as_str(), ".xls" | ".xla") && has_ole_signature {
        return Ok(());
    }

    let mut archive = match zip::ZipArchive::new(reader) {
        Ok(archive) => archive,
        Err(_) => return Ok(()),
    };
    if archive.len() > limits.max_files_in_archive {
        return Err(XbergError::validation(format!(
            "Spreadsheet ZIP archive declares {} entries, which exceeds the configured limit of {} \
             (SecurityLimits::max_files_in_archive); reduce the archive's entry count or raise the limit",
            archive.len(),
            limits.max_files_in_archive
        )));
    }
    crate::extractors::security::ZipBombValidator::new(limits.clone()).validate(&mut archive)?;
    Ok(())
}

pub(crate) fn read_excel_file(file_path: &str, limits: &SecurityLimits) -> Result<ExcelReadResult> {
    let lower_path = file_path.to_lowercase();
    let mut warnings: Vec<ProcessingWarning> = Vec::new();

    #[cfg(feature = "excel")]
    {
        let check_file = std::fs::File::open(file_path)?;
        let file_extension = Path::new(file_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!(".{extension}"))
            .unwrap_or_default();
        validate_zip_container(std::io::BufReader::new(check_file), &file_extension, limits)?;
    }
    #[cfg(not(feature = "excel"))]
    let _ = limits;

    #[cfg(feature = "office")]
    let office_metadata = if lower_path.ends_with(".xlsx")
        || lower_path.ends_with(".xlsm")
        || lower_path.ends_with(".xlam")
        || lower_path.ends_with(".xltm")
    {
        extract_xlsx_office_metadata_from_file(file_path).ok()
    } else if lower_path.ends_with(".ods") {
        extract_ods_office_metadata_from_file(file_path).ok()
    } else {
        None
    };

    #[cfg(not(feature = "office"))]
    let office_metadata: Option<HashMap<String, String>> = None;

    if lower_path.ends_with(".xlsx") || lower_path.ends_with(".xlsm") || lower_path.ends_with(".xltm") {
        return read_xlsx_family_file(file_path, office_metadata, warnings);
    }
    if lower_path.ends_with(".xlam") {
        return read_xlam_file(file_path, office_metadata, warnings);
    }
    if lower_path.ends_with(".xla") {
        return read_xla_file(file_path, office_metadata, warnings);
    }
    if lower_path.ends_with(".xlsb") {
        return read_xlsb_file(file_path, office_metadata, warnings);
    }

    let workbook = match open_workbook_auto(Path::new(file_path)) {
        Ok(wb) => wb,
        Err(calamine::Error::Io(io_err)) => {
            if io_err.kind() == std::io::ErrorKind::InvalidData {
                return Err(XbergError::parsing(format!(
                    "Cannot detect Excel file format: {}",
                    io_err
                )));
            }
            // Real IO error - bubble up unchanged ~keep
            return Err(io_err.into());
        }
        Err(e) => return Err(XbergError::parsing(format!("Failed to parse Excel file: {}", e))),
    };

    let result = process_workbook(workbook, office_metadata, &mut warnings)?;
    Ok((result, warnings))
}

/// Read an XLSX/XLSM/XLTM file: parse via calamine's XLSX reader, then merge sheet
/// revisions and comments — the extra sidecar data only this primary XLSX-family format
/// carries (XLAM shares the same reader but not this sidecar data, see [`read_xlam_file`]).
fn read_xlsx_family_file(
    file_path: &str,
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let file = std::fs::File::open(file_path)?;
    let workbook = calamine::Xlsx::new(std::io::BufReader::new(file))
        .map_err(|e| XbergError::parsing(format!("Failed to parse XLSX: {}", e)))?;
    let mut result = process_xlsx_workbook(workbook, office_metadata, &mut warnings)?;
    result.revisions = extract_xlsx_revisions_from_file(file_path);
    if let Some(comments) = extract_xlsx_comments_from_file(file_path) {
        result.metadata.insert("comments".to_owned(), comments);
    }
    Ok((result, warnings))
}

/// Read a `.xlam` add-in file via the XLSX reader; on failure return an empty workbook with
/// a warning instead of failing the whole read (an add-in commonly carries no sheet data
/// worth extracting, so a parse failure here is not fatal).
fn read_xlam_file(
    file_path: &str,
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let file = std::fs::File::open(file_path)?;
    match calamine::Xlsx::new(std::io::BufReader::new(file)) {
        Ok(workbook) => {
            let result = process_xlsx_workbook(workbook, office_metadata, &mut warnings)?;
            Ok((result, warnings))
        }
        Err(e) => {
            push_warning(
                &mut warnings,
                "excel",
                format!("Workbook could not be parsed as XLSX and no sheets were extracted ({e})"),
            );
            Ok((
                ExcelWorkbook {
                    sheets: vec![],
                    metadata: office_metadata.unwrap_or_default(),
                    revisions: None,
                },
                warnings,
            ))
        }
    }
}

/// Read a legacy `.xla` add-in file via the XLS reader, with the same
/// fall-back-to-empty-workbook-on-parse-failure behavior as [`read_xlam_file`].
fn read_xla_file(
    file_path: &str,
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let file = std::fs::File::open(file_path)?;
    match calamine::Xls::new(std::io::BufReader::new(file)) {
        Ok(workbook) => {
            let result = process_workbook(workbook, office_metadata, &mut warnings)?;
            Ok((result, warnings))
        }
        Err(e) => {
            push_warning(
                &mut warnings,
                "excel",
                format!("Workbook could not be parsed as XLS and no sheets were extracted ({e})"),
            );
            Ok((
                ExcelWorkbook {
                    sheets: vec![],
                    metadata: office_metadata.unwrap_or_default(),
                    revisions: None,
                },
                warnings,
            ))
        }
    }
}

/// Read a `.xlsb` binary workbook file.
fn read_xlsb_file(
    file_path: &str,
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let file = std::fs::File::open(file_path)?;
    let workbook = calamine::Xlsb::new(std::io::BufReader::new(file))
        .map_err(|e| XbergError::parsing(format!("Failed to parse XLSB: {}", e)))?;
    let result = process_workbook(workbook, office_metadata, &mut warnings)?;
    Ok((result, warnings))
}

pub(crate) fn read_excel_bytes(data: &[u8], file_extension: &str, limits: &SecurityLimits) -> Result<ExcelReadResult> {
    let warnings: Vec<ProcessingWarning> = Vec::new();

    #[cfg(feature = "excel")]
    validate_zip_container(Cursor::new(data), file_extension, limits)?;
    #[cfg(not(feature = "excel"))]
    let _ = limits;

    #[cfg(feature = "office")]
    let office_metadata = match file_extension.to_lowercase().as_str() {
        ".xlsx" | ".xlsm" | ".xlam" | ".xltm" => extract_xlsx_office_metadata_from_bytes(data).ok(),
        ".ods" => extract_ods_office_metadata_from_bytes(data).ok(),
        _ => None,
    };

    #[cfg(not(feature = "office"))]
    let office_metadata: Option<HashMap<String, String>> = None;

    match file_extension.to_lowercase().as_str() {
        ".xlsx" | ".xlsm" | ".xltm" => read_xlsx_family_bytes(data, office_metadata, warnings),
        ".xlam" => read_xlam_bytes(data, office_metadata, warnings),
        ".xls" => read_xls_bytes(data, office_metadata, warnings),
        ".xla" => read_xla_bytes(data, office_metadata, warnings),
        ".xlsb" => read_xlsb_bytes(data, office_metadata, warnings),
        ".ods" => read_ods_bytes(data, office_metadata, warnings),
        _ => Err(XbergError::parsing(format!(
            "Unsupported file extension: {}",
            file_extension
        ))),
    }
}

/// Read XLSX/XLSM/XLTM bytes: the byte-slice counterpart of [`read_xlsx_family_file`].
fn read_xlsx_family_bytes(
    data: &[u8],
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let cursor = Cursor::new(data);
    let workbook =
        calamine::Xlsx::new(cursor).map_err(|e| XbergError::parsing(format!("Failed to parse XLSX: {}", e)))?;
    let mut result = process_xlsx_workbook(workbook, office_metadata, &mut warnings)?;
    result.revisions = extract_xlsx_revisions_from_bytes(data);
    if let Some(comments) = extract_xlsx_comments_from_bytes(data) {
        result.metadata.insert("comments".to_owned(), comments);
    }
    Ok((result, warnings))
}

/// Read `.xlam` bytes: the byte-slice counterpart of [`read_xlam_file`].
fn read_xlam_bytes(
    data: &[u8],
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let cursor = Cursor::new(data);
    match calamine::Xlsx::new(cursor) {
        Ok(workbook) => {
            let result = process_xlsx_workbook(workbook, office_metadata, &mut warnings)?;
            Ok((result, warnings))
        }
        Err(e) => {
            push_warning(
                &mut warnings,
                "excel",
                format!("Workbook could not be parsed as XLSX and no sheets were extracted ({e})"),
            );
            Ok((
                ExcelWorkbook {
                    sheets: vec![],
                    metadata: office_metadata.unwrap_or_default(),
                    revisions: None,
                },
                warnings,
            ))
        }
    }
}

/// Read `.xls` bytes (no add-in fallback — see [`read_xla_bytes`] for the add-in variant).
fn read_xls_bytes(
    data: &[u8],
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let cursor = Cursor::new(data);
    let workbook =
        calamine::Xls::new(cursor).map_err(|e| XbergError::parsing(format!("Failed to parse XLS: {}", e)))?;
    let result = process_workbook(workbook, office_metadata, &mut warnings)?;
    Ok((result, warnings))
}

/// Read `.xla` bytes: the byte-slice counterpart of [`read_xla_file`].
fn read_xla_bytes(
    data: &[u8],
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let cursor = Cursor::new(data);
    match calamine::Xls::new(cursor) {
        Ok(workbook) => {
            let result = process_workbook(workbook, office_metadata, &mut warnings)?;
            Ok((result, warnings))
        }
        Err(e) => {
            push_warning(
                &mut warnings,
                "excel",
                format!("Workbook could not be parsed as XLS and no sheets were extracted ({e})"),
            );
            Ok((
                ExcelWorkbook {
                    sheets: vec![],
                    metadata: office_metadata.unwrap_or_default(),
                    revisions: None,
                },
                warnings,
            ))
        }
    }
}

/// Read `.xlsb` bytes: the byte-slice counterpart of [`read_xlsb_file`].
fn read_xlsb_bytes(
    data: &[u8],
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let cursor = Cursor::new(data);
    let workbook =
        calamine::Xlsb::new(cursor).map_err(|e| XbergError::parsing(format!("Failed to parse XLSB: {}", e)))?;
    let result = process_workbook(workbook, office_metadata, &mut warnings)?;
    Ok((result, warnings))
}

/// Read `.ods` bytes. `read_excel_file` has no direct counterpart — an on-disk `.ods` falls
/// through to `open_workbook_auto`, which dispatches to the same `calamine::Ods` reader.
fn read_ods_bytes(
    data: &[u8],
    office_metadata: Option<HashMap<String, String>>,
    mut warnings: Vec<ProcessingWarning>,
) -> Result<ExcelReadResult> {
    let cursor = Cursor::new(data);
    let workbook =
        calamine::Ods::new(cursor).map_err(|e| XbergError::parsing(format!("Failed to parse ODS: {}", e)))?;
    let result = process_workbook(workbook, office_metadata, &mut warnings)?;
    Ok((result, warnings))
}
