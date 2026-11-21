//! Delimited text carries a rectangular block of displayed values.
use crate::{book::*, storage};
use std::path::Path;

pub fn delimiter(name: &str) -> Result<u8> {
    match name {
        "tab" | "tsv" => Ok(b'\t'),
        "virgula" | "csv" => Ok(b','),
        "ponto-e-virgula" => Ok(b';'),
        _ => Err("Escolha tab, virgula ou ponto-e-virgula".into()),
    }
}

fn check_path(path: &Path) -> Result {
    if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("csv") || s.eq_ignore_ascii_case("tsv"))
    {
        return Ok(());
    }
    Err("Use .csv ou .tsv para uma única aba ou seleção, sem estilos".into())
}

pub fn read(path: &Path, encoding: &str) -> Result<String> {
    check_path(path)?;
    let bytes = storage::bounded_read(path)?;
    match encoding {
        "utf-8" => String::from_utf8(bytes)
            .map(|s| s.trim_start_matches('\u{feff}').into())
            .map_err(|_| "Texto UTF-8 inválido".into()),
        "latin1" => Ok(bytes.into_iter().map(char::from).collect()),
        _ => Err("Codificações disponíveis: utf-8 e latin1".into()),
    }
}

/// csv skips empty physical records. Quote blank records outside multiline
/// fields so that an empty spreadsheet row retains its position when pasted.
fn preserve_empty_records(input: &str, delimiter: u8) -> Vec<u8> {
    let mut bytes = input.as_bytes().iter().copied().peekable();
    let mut output = Vec::with_capacity(input.len());
    let mut quoted = false;
    let mut field_start = true;
    let mut record_start = true;
    while let Some(b) = bytes.next() {
        if quoted {
            output.push(b);
            if b == b'"' {
                if bytes.next_if_eq(&b'"').is_some() {
                    output.push(b'"')
                } else {
                    quoted = false
                }
            }
        } else if matches!(b, b'\r' | b'\n') {
            if record_start {
                output.extend_from_slice(b"\"\"")
            }
            output.push(b);
            if b == b'\r' && bytes.next_if_eq(&b'\n').is_some() {
                output.push(b'\n')
            }
            field_start = true;
            record_start = true;
        } else {
            quoted = field_start && b == b'"';
            field_start = b == delimiter;
            record_start = false;
            output.push(b);
        }
    }
    output
}

pub fn paste(book: &mut Book, sheet: u32, row: i32, col: i32, text: &str, delimiter: u8) -> Result {
    let input = preserve_empty_records(text, delimiter);
    let mut csv = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(input.as_slice());
    let mut rows = Vec::new();
    let mut width = 0;
    for record in csv.records() {
        let record = record.map_err(|e| e.to_string())?;
        width = width.max(record.len());
        let area = Range {
            r: row,
            c: col,
            end: row + rows.len() as i32,
            right: col + width as i32 - 1,
        };
        area.bounded()?;
        rows.push(record);
    }
    for (i, record) in rows.iter().enumerate() {
        for j in 0..width {
            book.set(
                sheet,
                row + i as i32,
                col + j as i32,
                record.get(j).unwrap_or(""),
            )?;
        }
    }
    Ok(())
}

pub fn export(
    book: &Book,
    sheet: u32,
    area: Range,
    path: &Path,
    delimiter: u8,
    encoding: &str,
) -> Result {
    check_path(path)?;
    area.bounded()?;
    let mut csv = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    for r in area.r..=area.end {
        csv.write_record((area.c..=area.right).map(|c| book.val(sheet, r, c)))
            .map_err(|e| e.to_string())?;
    }
    let utf8 = csv.into_inner().map_err(|e| e.to_string())?;
    let bytes = match encoding {
        "utf-8" => utf8,
        "latin1" => std::str::from_utf8(&utf8)
            .map_err(|e| e.to_string())?
            .chars()
            .map(|c| u8::try_from(c as u32).map_err(|_| "Caractere fora de Latin-1".to_owned()))
            .collect::<Result<Vec<u8>>>()?,
        _ => return Err("Codificações disponíveis: utf-8 e latin1".into()),
    };
    storage::atomic(path, &bytes, false, None)
}
