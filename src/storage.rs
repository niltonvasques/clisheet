//! Only decorate XML emitted by this exact exporter. External packages with unsupported features require a new path.
mod import;
mod notes;
use crate::{book::*, data::*};
use ironcalc::{
    base::{Model, UserModel},
    export::save_xlsx_to_writer,
    import::load_from_xlsx_bytes,
};
use quick_xml::{
    events::{BytesText, Event},
    Reader, Writer,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};
type Parts = BTreeMap<String, Vec<u8>>;
const MAX: u64 = 256 << 20;
const META: &str = "customXml/clisheet.xml";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn esc(v: &str) -> String {
    quick_xml::escape::escape(v).into_owned()
}
fn xml(parts: &Parts, k: &str) -> Result<String> {
    String::from_utf8(
        parts
            .get(k)
            .ok_or_else(|| format!("Parte ausente: {k}"))?
            .clone(),
    )
    .map_err(err)
}
fn ext(path: &Path) -> Result {
    if path
        .extension()
        .is_some_and(|v| v.eq_ignore_ascii_case("xlsx"))
    {
        Ok(())
    } else {
        Err("Use um arquivo .xlsx sem macros e sem criptografia".into())
    }
}
pub(crate) fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(err)?;
    if file.metadata().map_err(err)?.len() > MAX {
        return Err("Arquivo excede 256 MiB".into());
    }
    let mut out = Vec::new();
    file.take(MAX + 1).read_to_end(&mut out).map_err(err)?;
    if out.len() as u64 > MAX {
        return Err("Arquivo excede 256 MiB".into());
    }
    Ok(out)
}
fn parts(bytes: &[u8]) -> Result<Parts> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("XLSX inválido ou criptografado: {e}"))?;
    if zip.len() > 4096 {
        return Err("Número excessivo de partes".into());
    }
    let mut out = Parts::new();
    let mut left = MAX;
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(err)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_owned();
        if file.size() > left || file.size() > 128 << 20 {
            return Err("Limite de descompactação excedido".into());
        }
        let size = file.size();
        let mut part = Vec::new();
        file.take(size + 1).read_to_end(&mut part).map_err(err)?;
        if part.len() as u64 != size {
            return Err("Tamanho ZIP inválido".into());
        }
        left -= size;
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let mut reader = Reader::from_reader(&part[..]);
            let mut depth = 0u32;
            let mut array_cells = 0i64;
            loop {
                let event = reader.read_event().map_err(err)?;
                if let Event::Start(t) | Event::Empty(t) = &event {
                    if t.local_name().as_ref() == b"f"
                        && t.try_get_attribute("t")
                            .map_err(err)?
                            .is_some_and(|a| a.value.as_ref() == b"array")
                    {
                        let reference = t
                            .try_get_attribute("ref")
                            .map_err(err)?
                            .ok_or("Matriz sem intervalo")?;
                        let a = parse_range(std::str::from_utf8(&reference.value).map_err(err)?)?;
                        array_cells += i64::from(a.end - a.r + 1) * i64::from(a.right - a.c + 1);
                        if array_cells > BATCH {
                            return Err(
                                "Matrizes excedem o limite de 500 mil células por aba".into()
                            );
                        }
                    }
                }
                match event {
                    Event::Start(_) => {
                        depth += 1;
                        if depth > 128 {
                            return Err("XML muito profundo".into());
                        }
                    }
                    Event::End(_) => depth = depth.saturating_sub(1),
                    Event::DocType(_) => return Err("DOCTYPE recusado".into()),
                    Event::Eof => break,
                    _ => (),
                }
            }
        }
        if out.insert(name, part).is_some() {
            return Err("Parte ZIP duplicada".into());
        }
    }
    Ok(out)
}
fn fingerprint(parts: &Parts) -> String {
    let mut h = Sha256::new();
    for (k, v) in parts {
        h.update((k.len() as u64).to_le_bytes());
        h.update(k);
        h.update((v.len() as u64).to_le_bytes());
        h.update(v)
    }
    format!("clisheet1:{:x}", h.finalize())
}
fn pack(parts: &Parts) -> Result<Vec<u8>> {
    let mut z = ZipWriter::new(Cursor::new(Vec::new()));
    z.set_comment(fingerprint(parts));
    for (k, v) in parts {
        z.start_file(k, SimpleFileOptions::default()).map_err(err)?;
        z.write_all(v).map_err(err)?
    }
    Ok(z.finish().map_err(err)?.into_inner())
}
fn append(parts: &mut Parts, k: &str, closing: &str, fragment: &str) -> Result {
    let text = xml(parts, k)?;
    let pos = text.rfind(closing).ok_or("XML gerado inesperado")?;
    let mut value = text;
    value.insert_str(pos, fragment);
    parts.insert(k.into(), value.into_bytes());
    Ok(())
}
fn formulas_xml(sheet: &str, expr: &BTreeMap<String, String>) -> Result<String> {
    let mut reader = Reader::from_str(sheet);
    let mut writer = Writer::new(Vec::new());
    let mut current = None;
    let mut replacing = false;
    loop {
        let e = reader.read_event().map_err(err)?;
        match &e {
            Event::Start(t) if t.name().as_ref() == b"c" => {
                current = t
                    .try_get_attribute("r")
                    .map_err(err)?
                    .and_then(|a| expr.get(std::str::from_utf8(&a.value).ok()?))
            }
            Event::Start(t) if t.name().as_ref() == b"f" && current.is_some() => {
                writer.write_event(e.borrow()).map_err(err)?;
                writer
                    .write_event(Event::Text(BytesText::new(current.unwrap())))
                    .map_err(err)?;
                replacing = true;
                continue;
            }
            Event::End(t) if t.name().as_ref() == b"f" => replacing = false,
            Event::End(t) if t.name().as_ref() == b"c" => current = None,
            Event::Eof => break,
            _ => (),
        }
        if !replacing {
            writer.write_event(e).map_err(err)?
        }
    }
    String::from_utf8(writer.into_inner()).map_err(err)
}
fn filter_xml(f: &Filter, area: Range) -> String {
    let op = [
        (">=", "greaterThanOrEqual"),
        ("<=", "lessThanOrEqual"),
        ("<>", "notEqual"),
        (">", "greaterThan"),
        ("<", "lessThan"),
        ("=", "equal"),
        ("~", "equal"),
    ]
    .into_iter()
    .find(|(s, _)| f.condition.starts_with(s));
    let criteria = if f.condition == "vazio" {
        "<filters blank=\"1\"/>".into()
    } else if let Some((p, op)) = op {
        let raw = &f.condition[p.len()..];
        let v = if p == "~" {
            format!("*{raw}*")
        } else {
            raw.into()
        };
        format!(
            "<customFilters><customFilter operator=\"{op}\" val=\"{}\"/></customFilters>",
            esc(&v)
        )
    } else {
        format!(
            "<filters>{}</filters>",
            f.condition
                .split('|')
                .map(|v| format!("<filter val=\"{}\"/>", esc(v)))
                .collect::<String>()
        )
    };
    format!(
        "<autoFilter ref=\"{}\"><filterColumn colId=\"{}\">{criteria}</filterColumn></autoFilter>",
        area.address(),
        f.col - area.c
    )
}
fn table_xml(t: &Table, id: usize, filter: Option<&Filter>) -> String {
    let a = t.area;
    let data = Range {
        end: a.end - i32::from(t.total),
        ..a
    };
    let filter = filter
        .map(|f| filter_xml(f, data))
        .unwrap_or_else(|| format!("<autoFilter ref=\"{}\"/>", data.address()));
    let cols = t
        .headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let c = a.c + i as i32;
            let calc = t
                .formulas
                .get(&c)
                .map(|v| {
                    format!(
                        "<calculatedColumnFormula>{}</calculatedColumnFormula>",
                        esc(v.trim_start_matches('='))
                    )
                })
                .unwrap_or_default();
            let total = if t.total && i == 0 {
                " totalsRowLabel=\"Total\""
            } else if t.total && t.formulas.contains_key(&c) {
                " totalsRowFunction=\"sum\""
            } else {
                ""
            };
            format!(
                "<tableColumn id=\"{}\" name=\"{}\"{total}>{calc}</tableColumn>",
                i + 1,
                esc(h)
            )
        })
        .collect::<String>();
    let name = esc(&t.name);
    format!("<table xmlns=\"{NS}\" id=\"{id}\" name=\"{name}\" displayName=\"{name}\" ref=\"{}\" totalsRowCount=\"{}\">{filter}<tableColumns count=\"{}\">{cols}</tableColumns><tableStyleInfo name=\"TableStyleMedium2\" showRowStripes=\"1\"/></table>",a.address(),u8::from(t.total),t.headers.len())
}
fn validation_xml(rule: &Rule) -> String {
    let (t, lo, hi) = match rule.kind.as_str() {
        "lista" => ("list", format!("\"{}\"", rule.args.join(",")), None),
        "data" => {
            let date = |v: &str| format!("DATE({})", v.replace('-', ","));
            ("date", date(&rule.args[0]), Some(date(&rule.args[1])))
        }
        _ => (
            "decimal",
            rule.args[0].replace(',', "."),
            Some(rule.args[1].replace(',', ".")),
        ),
    };
    format!("<dataValidation type=\"{t}\" allowBlank=\"1\" showErrorMessage=\"1\" sqref=\"{}\"><formula1>{}</formula1>{}</dataValidation>",rule.area.address(),esc(&lo),hi.map(|v|format!("<formula2>{}</formula2>",esc(&v))).unwrap_or_default())
}
pub fn encode(book: &Book) -> Result<Vec<u8>> {
    let base = save_xlsx_to_writer(book.m.get_model(), Cursor::new(Vec::new()))
        .map_err(err)?
        .into_inner();
    let mut files = parts(&base)?;
    let locale = book.m.get_locale();
    for (s, ws) in book.m.get_worksheets_properties().iter().enumerate() {
        let key = format!("xl/worksheets/sheet{}.xml", s + 1);
        let rels = format!("xl/worksheets/_rels/sheet{}.xml.rels", s + 1);
        let mut sheet = xml(&files, &key)?;
        let mut expr = BTreeMap::new();
        for e in book
            .extra
            .expressions
            .iter()
            .filter(|e| e.sheet == ws.sheet_id)
        {
            let canonical =
                crate::structure::dialect(book, s as u32, e.r, e.c, &e.text, &locale, "xlsx")?;
            expr.insert(addr(e.r, e.c), canonical[1..].to_owned());
        }
        if !expr.is_empty() {
            sheet = formulas_xml(&sheet, &expr)?
        }
        let filter = book.extra.filters.iter().find(|f| f.sheet == ws.sheet_id);
        let mut tableparts = Vec::new();
        let mut inside = false;
        for (i, t) in book
            .extra
            .tables
            .iter()
            .enumerate()
            .filter(|(_, t)| t.sheet == ws.sheet_id)
        {
            let id = i + 1;
            let table_filter = filter.filter(|f| f.area == t.area);
            inside |= table_filter.is_some();
            let mut exported = t.clone();
            for (&col, f) in &mut exported.formulas {
                *f = crate::structure::dialect(
                    book,
                    s as u32,
                    t.area.r + 1,
                    col,
                    f,
                    &locale,
                    "xlsx",
                )?;
            }
            files.insert(
                format!("xl/tables/table{id}.xml"),
                table_xml(&exported, id, table_filter).into_bytes(),
            );
            append(&mut files,"[Content_Types].xml","</Types>",&format!("<Override PartName=\"/xl/tables/table{id}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml\"/>"))?;
            files.entry(rels.clone()).or_insert_with(||b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>".to_vec());
            append(&mut files,&rels,"</Relationships>",&format!("<Relationship Id=\"table{id}\" Type=\"{REL}/table\" Target=\"/xl/tables/table{id}.xml\"/>"))?;
            tableparts.push(format!("<tablePart r:id=\"table{id}\"/>"));
        }
        if let Some(f) = filter.filter(|_| !inside) {
            sheet = sheet.replacen(
                "</sheetData>",
                &format!("</sheetData>{}", filter_xml(f, f.area)),
                1,
            )
        }
        let rules: Vec<_> = book
            .extra
            .rules
            .iter()
            .filter(|r| r.sheet == ws.sheet_id)
            .collect();
        if !rules.is_empty() {
            let pos = ["<hyperlinks", "<extLst", "</worksheet>"]
                .iter()
                .filter_map(|k| sheet.find(k))
                .min()
                .ok_or("XML inválido")?;
            sheet.insert_str(
                pos,
                &format!(
                    "<dataValidations count=\"{}\">{}</dataValidations>",
                    rules.len(),
                    rules.into_iter().map(validation_xml).collect::<String>()
                ),
            )
        }
        if !tableparts.is_empty() {
            let pos = sheet
                .find("<extLst")
                .or_else(|| sheet.find("</worksheet>"))
                .ok_or("XML inválido")?;
            sheet.insert_str(
                pos,
                &format!(
                    "<tableParts count=\"{}\">{}</tableParts>",
                    tableparts.len(),
                    tableparts.join("")
                ),
            )
        }
        notes::write_notes(
            &mut files,
            &mut sheet,
            &rels,
            s + 1,
            &book.ws(s as u32).comments,
        )?;
        files.insert(key, sheet.into_bytes());
    }
    let mut standard = book.extra.clone();
    for e in &mut standard.expressions {
        let s = book
            .m
            .get_worksheets_properties()
            .iter()
            .position(|w| w.sheet_id == e.sheet)
            .ok_or("Aba ausente")?;
        e.text = crate::structure::dialect(book, s as u32, e.r, e.c, &e.text, &locale, "xlsx")?;
    }
    for t in &mut standard.tables {
        let s = book
            .m
            .get_worksheets_properties()
            .iter()
            .position(|w| w.sheet_id == t.sheet)
            .ok_or("Aba ausente")?;
        for (&c, f) in &mut t.formulas {
            *f = crate::structure::dialect(book, s as u32, t.area.r + 1, c, f, &locale, "xlsx")?;
        }
    }
    let meta = serde_json::to_string(&standard).map_err(err)?;
    files.insert(
        META.into(),
        format!(
            "<clisheet xmlns=\"urn:clisheet:1\" formulaDialect=\"xlsx\">{}</clisheet>",
            esc(&meta)
        )
        .into_bytes(),
    );
    append(
        &mut files,
        "xl/_rels/workbook.xml.rels",
        "</Relationships>",
        &format!(
            "<Relationship Id=\"clisheetMeta\" Type=\"{REL}/customXml\" Target=\"/{}\"/>",
            META
        ),
    )?;
    let wb = xml(&files, "xl/workbook.xml")?.replace("<calcPr/>", "<calcPr fullCalcOnLoad=\"1\"/>");
    files.insert("xl/workbook.xml".into(), wb.into_bytes());
    pack(&files)
}
/// Common grid-only packages can use the original path. Unrecognized parts or
/// worksheet features keep the conservative Save Copy policy.
fn plain_grid_package(files: &Parts) -> bool {
    for (path, bytes) in files {
        let tags = if path == "xl/workbook.xml" {
            "workbook fileVersion workbookPr workbookProtection bookViews workbookView sheets sheet definedNames definedName calcPr"
        } else if path
            .strip_prefix("xl/worksheets/sheet")
            .and_then(|s| s.strip_suffix(".xml"))
            .is_some_and(|n| n.parse::<u32>().is_ok())
        {
            "worksheet sheetPr outlinePr pageSetUpPr dimension sheetViews sheetView pane selection sheetFormatPr cols col sheetData row c v f is t mergeCells mergeCell pageMargins"
        } else if path == "xl/sharedStrings.xml" {
            "sst si t"
        } else if path == "xl/styles.xml" {
            "styleSheet numFmts numFmt fonts font name sz color family scheme b i u strike fills fill patternFill fgColor bgColor borders border left right top bottom diagonal cellStyleXfs cellXfs xf alignment cellStyles cellStyle tableStyles dxfs colors indexedColors rgbColor"
        } else if path == "_rels/.rels" || path == "xl/_rels/workbook.xml.rels" {
            "Relationships Relationship"
        } else if matches!(
            path.as_str(),
            "[Content_Types].xml"
                | "docProps/app.xml"
                | "docProps/core.xml"
                | "xl/theme/theme1.xml"
                | "xl/metadata.xml"
        ) {
            // Only accept the engine's known dynamic-array metadata.
            if path == "xl/metadata.xml"
                && !bytes.eq(&*include_bytes!("storage/default_metadata.xml"))
            {
                return false;
            }
            continue;
        } else {
            return false;
        };
        let mut reader = Reader::from_reader(bytes.as_slice());
        loop {
            match reader.read_event() {
                Ok(Event::Start(t) | Event::Empty(t)) => {
                    if !tags
                        .split_whitespace()
                        .any(|name| name.as_bytes() == t.name().as_ref())
                    {
                        return false;
                    }
                    if t.name().as_ref() == b"workbookProtection" && t.attributes().next().is_some()
                    {
                        return false;
                    }
                    if t.name().as_ref() == b"Relationship" {
                        let Ok(Some(kind)) = t.try_get_attribute("Type") else {
                            return false;
                        };
                        let end = kind.value.rsplit(|&b| b == b'/').next().unwrap_or_default();
                        if !matches!(
                            end,
                            b"officeDocument"
                                | b"core-properties"
                                | b"extended-properties"
                                | b"worksheet"
                                | b"theme"
                                | b"styles"
                                | b"sharedStrings"
                                | b"sheetMetadata"
                        ) {
                            return false;
                        }
                    }
                    if t.name().as_ref() == b"Relationship"
                        && t.try_get_attribute("TargetMode").ok().flatten().is_some()
                    {
                        return false;
                    }
                    if t.name().as_ref() == b"f"
                        && t.try_get_attribute("t")
                            .ok()
                            .flatten()
                            .is_some_and(|v| !matches!(v.value.as_ref(), b"normal" | b"shared"))
                    {
                        return false;
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => return false,
                _ => (),
            }
        }
    }
    true
}
fn decode(bytes: &[u8], locale: &str) -> Result<Book> {
    let files = parts(bytes)?;
    if files
        .keys()
        .any(|k| k.to_ascii_lowercase().contains("vbaproject"))
        || xml(&files, "[Content_Types].xml")?.contains("macroEnabled")
    {
        return Err("Pasta com macros recusada".into());
    }
    let workbook = xml(&files, "xl/workbook.xml")?;
    let mut reader = Reader::from_str(&workbook);
    loop {
        match reader.read_event().map_err(err)? {
            Event::Empty(t) | Event::Start(t) if t.local_name().as_ref() == b"workbookPr" => {
                if t.try_get_attribute("date1904")
                    .map_err(err)?
                    .is_some_and(|a| matches!(a.value.as_ref(), b"1" | b"true"))
                {
                    return Err("Sistema de datas 1904 identificado. Abertura bloqueada para preservar datas".into());
                }
            }
            Event::Eof => break,
            _ => (),
        }
    }
    let owned = ZipArchive::new(Cursor::new(bytes)).map_err(err)?.comment()
        == fingerprint(&files).as_bytes();
    let mut wb =
        load_from_xlsx_bytes(bytes, "Pasta", engine_locale(locale)?, "UTC").map_err(err)?;
    if wb.worksheets.is_empty()
        || !wb
            .worksheets
            .iter()
            .any(|w| w.state.to_string() == "visible")
    {
        return Err("Nenhuma aba visível".into());
    }
    let mut extras = Extras::default();
    if owned {
        let meta = xml(&files, META)?;
        let mut r = Reader::from_str(&meta);
        let canonical = match r.read_event().map_err(err)? {
            Event::Start(t) => t
                .try_get_attribute("formulaDialect")
                .map_err(err)?
                .is_some_and(|a| a.value.as_ref() == b"xlsx"),
            _ => return Err("Metadados inválidos".into()),
        };
        let raw = r
            .read_text(quick_xml::name::QName(b"clisheet"))
            .map_err(err)?;
        extras =
            serde_json::from_str(&quick_xml::escape::unescape(&raw).map_err(err)?).map_err(err)?;
        // Earlier packages kept the editing locale in customXml. Their worksheet
        // formulas and table parts already carry canonical Excel expressions.
        if !canonical && !extras.tables.is_empty() {
            let mut imported = Extras {
                tables: extras.tables.clone(),
                ..Default::default()
            };
            import::extras(&files, &mut imported)?;
            extras.expressions = imported.expressions;
            extras.tables = imported.tables;
        }
    } else {
        for t in wb.tables.values() {
            if let Some(w) = wb.worksheets.iter().find(|w| w.name == t.sheet_name) {
                extras.tables.push(Table {
                    name: t.name.clone(),
                    sheet_name: w.name.clone(),
                    sheet: w.sheet_id,
                    area: parse_range(&t.reference)?,
                    headers: t.columns.iter().map(|c| c.name.clone()).collect(),
                    formulas: BTreeMap::new(),
                    total: t.totals_row_count != 0,
                })
            }
        }
    }
    if !owned {
        import::extras(&files, &mut extras)?;
    }
    let ids: HashSet<_> = wb.worksheets.iter().map(|w| w.sheet_id).collect();
    for t in &extras.tables {
        t.area.bounded()?;
        if !ids.contains(&t.sheet) || t.headers.len() as i32 != t.area.right - t.area.c + 1 {
            return Err("Tabela inconsistente".into());
        }
    }
    for r in &extras.rules {
        r.area.bounded()?;
        if r.kind != "lista" && r.args.len() != 2 {
            return Err("Validação inválida".into());
        }
    }
    import::note_authors(&files, &mut wb)?;
    wb.settings.locale = engine_locale(locale)?.into();
    let m = Model::from_workbook(wb, "en")?;
    let mut b = Book::from_model(UserModel::from_model(m));
    for e in &mut extras.expressions {
        let s =
            b.m.get_worksheets_properties()
                .iter()
                .position(|w| w.sheet_id == e.sheet)
                .ok_or("Aba ausente")?;
        e.text = crate::structure::dialect(
            &b,
            s as u32,
            e.r,
            e.c,
            &e.text,
            "en",
            engine_locale(locale)?,
        )?
    }
    for t in &mut extras.tables {
        let s =
            b.m.get_worksheets_properties()
                .iter()
                .position(|w| w.sheet_id == t.sheet)
                .ok_or("Aba ausente")?;
        for (&c, f) in &mut t.formulas {
            *f = crate::structure::dialect(
                &b,
                s as u32,
                t.area.r + 1,
                c,
                f,
                "en",
                engine_locale(locale)?,
            )?
        }
    }
    b.extra = extras;
    b.refresh_refs()?;
    b.finish();
    if !owned {
        for f in &mut b.extra.filters {
            let s =
                b.m.get_worksheets_properties()
                    .iter()
                    .position(|w| w.sheet_id == f.sheet)
                    .ok_or("Aba de filtro inválida")? as u32;
            f.area.bounded()?;
            for rr in f.area.r + 1..=f.area.end {
                if b.m.get_model().is_row_hidden(s, rr)? {
                    let value = b.m.get_formatted_cell_value(s, rr, f.col)?;
                    let number = match b.m.get_model().get_cell_value_by_index(s, rr, f.col)? {
                        ironcalc::base::cell::CellValue::Number(n) => Some(n),
                        _ => None,
                    };
                    if !crate::data::matches_condition(&value, number, &f.condition) {
                        f.hidden.insert(rr);
                        f.excluded.insert(rr);
                    }
                }
            }
        }
        b.finish()
    }
    b.copy_only = !owned && !plain_grid_package(&files);
    b.digest = Some(Sha256::digest(bytes).into());
    Ok(b)
}
pub fn open(path: &Path, locale: &str) -> Result<Book> {
    ext(path)?;
    let bytes = bounded_read(path)?;
    let mut b = std::panic::catch_unwind(|| decode(&bytes, locale))
        .map_err(|_| "Estrutura XLSX incompatível")??;
    b.path = Some(fs::canonicalize(path).map_err(err)?);
    Ok(b)
}
pub(crate) fn atomic(
    path: &Path,
    bytes: &[u8],
    replace: bool,
    permissions: Option<fs::Permissions>,
) -> Result {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(err)?;
    tmp.write_all(bytes).map_err(err)?;
    if let Some(permissions) = permissions {
        tmp.as_file().set_permissions(permissions).map_err(err)?;
    }
    tmp.as_file().sync_all().map_err(err)?;
    let actual = bounded_read(tmp.path())?;
    if actual != bytes {
        return Err("Verificação da gravação falhou".into());
    }
    if replace {
        tmp.persist(path).map_err(err)?
    } else {
        tmp.persist_noclobber(path).map_err(err)?
    };
    #[cfg(unix)]
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(err)?;
    Ok(())
}
pub fn save(b: &mut Book, path: &Path, backup: bool) -> Result {
    if b.readonly {
        return Err("Somente leitura".into());
    }
    ext(path)?;
    let same = b
        .path
        .as_ref()
        .is_some_and(|p| fs::canonicalize(path).ok().as_ref() == Some(p) || p == path);
    if same && b.copy_only {
        return Err("Preservação parcial. Use Salvar cópia em um caminho novo".into());
    }
    if !same && path.exists() {
        return Err("O destino já existe. Escolha um nome novo".into());
    }
    let bytes = encode(b)?;
    let _validated = decode(&bytes, &b.m.get_locale())?;
    let destination = if same {
        b.path.as_deref().unwrap()
    } else {
        path
    };
    // Serialize cooperating writers across the digest check and atomic rename.
    // A directory lock keeps the identity stable when the workbook is replaced.
    #[cfg(unix)]
    let _lock = if same {
        let directory =
            fs::File::open(destination.parent().ok_or("Destino inválido")?).map_err(err)?;
        directory.lock().map_err(err)?;
        Some(directory)
    } else {
        None
    };
    let permissions = if same {
        let original = bounded_read(destination)?;
        if b.digest != Some(Sha256::digest(&original).into()) {
            return Err("Arquivo alterado externamente. Salve uma cópia".into());
        }
        let permissions = fs::metadata(destination).map_err(err)?.permissions();
        if permissions.readonly() {
            return Err("Arquivo sem permissão de escrita. Salve uma cópia".into());
        }
        if backup {
            atomic(
                &destination.with_extension("xlsx.bak"),
                &original,
                true,
                None,
            )?
        }
        Some(permissions)
    } else {
        None
    };
    atomic(destination, &bytes, same, permissions)?;
    b.path = Some(fs::canonicalize(path).map_err(err)?);
    b.copy_only = false;
    b.digest = Some(Sha256::digest(bytes).into());
    b.saved = b.version;
    Ok(())
}
pub fn recovery_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
        })
        .join("clisheet")
}
pub fn checkpoint(book: &Book, path: &Path) -> Result {
    let dir = path.parent().ok_or("Caminho inválido")?;
    fs::create_dir_all(dir).map_err(err)?;
    atomic(path, &encode(book)?, true, None)
}
pub fn latest_recovery() -> Result<PathBuf> {
    fs::read_dir(recovery_dir())
        .map_err(|_| "Nenhuma sessão recuperável")?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "xlsx"))
        .max_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
        .ok_or("Nenhuma sessão recuperável".into())
}
