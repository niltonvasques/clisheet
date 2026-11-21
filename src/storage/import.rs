//! Standard OOXML metadata from workbooks created outside clisheet.
use super::*;
use quick_xml::events::BytesStart;
#[derive(Default)]
struct Element {
    tag: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<Element>,
}
fn element(t: &BytesStart) -> Result<Element> {
    let tag = String::from_utf8_lossy(t.local_name().as_ref()).into_owned();
    let mut attrs = BTreeMap::new();
    for a in t.attributes() {
        let a = a.map_err(err)?;
        let name = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        attrs.insert(name, a.unescape_value().map_err(err)?.into_owned());
    }
    Ok(Element {
        tag,
        attrs,
        ..Element::default()
    })
}
fn children(parent: &mut Element, r: &mut Reader<&[u8]>) -> Result {
    loop {
        match r.read_event().map_err(err)? {
            Event::Start(t) => {
                let mut child = element(&t)?;
                children(&mut child, r)?;
                parent.children.push(child)
            }
            Event::Empty(t) => parent.children.push(element(&t)?),
            Event::Text(t) => parent.text.push_str(&t.decode().map_err(err)?),
            Event::GeneralRef(t) => {
                let v = t.decode().map_err(err)?;
                parent
                    .text
                    .push_str(&quick_xml::escape::unescape(&format!("&{v};")).map_err(err)?)
            }
            Event::End(_) | Event::Eof => break,
            _ => (),
        }
    }
    Ok(())
}
impl Element {
    fn attr(&self, key: &str) -> &str {
        self.attrs.get(key).map_or("", String::as_str)
    }
    fn child(&self, tag: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.tag == tag)
    }
}
fn root(xml: &str) -> Result<Element> {
    let mut r = Reader::from_str(xml);
    let mut doc = Element::default();
    children(&mut doc, &mut r)?;
    doc.children.into_iter().next().ok_or("XML vazio".into())
}
fn relations(files: &Parts, key: &str, base: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for rel in root(&xml(files, key)?)?.children {
        if rel.attr("TargetMode") == "External" {
            continue;
        }
        let target = rel.attr("Target");
        let path = part_target(base, target)?;
        out.insert(rel.attr("Id").to_owned(), path);
    }
    Ok(out)
}
/// Resolve relative OPC relationship targets inside the archive.
fn part_target(base: &str, target: &str) -> Result<String> {
    let mut components = Vec::new();
    if !target.starts_with('/') {
        components.extend(base.split('/').filter(|s| !s.is_empty()));
    }
    for part in target.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                components
                    .pop()
                    .ok_or("Caminho de relacionamento inválido")?;
            }
            part => components.push(part),
        }
    }
    Ok(components.join("/"))
}

pub(super) fn note_authors(files: &Parts, wb: &mut ironcalc::base::types::Workbook) -> Result {
    if wb.worksheets.iter().all(|ws| ws.comments.is_empty()) {
        return Ok(());
    }
    let workbook = root(&xml(files, "xl/workbook.xml")?)?;
    let rels = relations(files, "xl/_rels/workbook.xml.rels", "xl")?;
    for sheet in &workbook.child("sheets").ok_or("Pasta sem abas")?.children {
        let id = sheet.attr("sheetId").parse::<u32>().map_err(err)?;
        let Some(ws) = wb
            .worksheets
            .iter_mut()
            .find(|w| w.sheet_id == id && !w.comments.is_empty())
        else {
            continue;
        };
        let key = rels
            .get(sheet.attr("r:id"))
            .ok_or("Relacionamento de aba ausente")?;
        let (directory, file) = key.rsplit_once('/').unwrap_or(("", key));
        let links = root(&xml(files, &format!("{directory}/_rels/{file}.rels"))?)?;
        let Some(link) = links
            .children
            .iter()
            .find(|l| l.attr("Type").ends_with("/comments") && l.attr("TargetMode") != "External")
        else {
            continue;
        };
        let notes = root(&xml(files, &part_target(directory, link.attr("Target"))?)?)?;
        let Some(authors) = notes.child("authors") else {
            continue;
        };
        let Some(list) = notes.child("commentList") else {
            continue;
        };
        let by_cell: BTreeMap<_, _> = list
            .children
            .iter()
            .filter_map(|n| {
                let author = authors
                    .children
                    .get(n.attr("authorId").parse::<usize>().ok()?)?;
                Some((n.attr("ref"), &author.text))
            })
            .collect();
        for note in &mut ws.comments {
            if let Some(name) = by_cell.get(note.cell_ref.as_str()) {
                note.author_name.clone_from(name)
            }
        }
    }
    Ok(())
}
fn rule(el: &Element, id: u32) -> Vec<Rule> {
    if !matches!(el.attr("operator"), "" | "between") {
        return vec![];
    }
    let Some(lo) = el.child("formula1") else {
        return vec![];
    };
    let (kind, args) = match el.attr("type") {
        "list" if lo.text.starts_with('"') && lo.text.ends_with('"') => (
            "lista",
            lo.text
                .trim_matches('"')
                .split(',')
                .map(str::to_owned)
                .collect(),
        ),
        "decimal" | "whole" | "date" => {
            let Some(hi) = el.child("formula2") else {
                return vec![];
            };
            if el.attr("type") == "date" {
                let parse = |s: &str| {
                    if let Ok(serial) = s.parse::<i64>() {
                        ironcalc::base::formatter::dates::from_excel_date(serial)
                            .ok()
                            .map(|d| d.to_string())
                    } else {
                        let parts = s
                            .strip_prefix("DATE(")?
                            .strip_suffix(')')?
                            .split(',')
                            .map(str::parse::<i32>)
                            .collect::<std::result::Result<Vec<_>, _>>()
                            .ok()?;
                        if parts.len() != 3 {
                            return None;
                        }
                        chrono::NaiveDate::from_ymd_opt(parts[0], parts[1] as u32, parts[2] as u32)
                            .map(|d| d.to_string())
                    }
                };
                let (Some(a), Some(b)) = (parse(&lo.text), parse(&hi.text)) else {
                    return vec![];
                };
                ("data", vec![a, b])
            } else {
                if lo.text.parse::<f64>().is_err() || hi.text.parse::<f64>().is_err() {
                    return vec![];
                }
                ("numero", vec![lo.text.clone(), hi.text.clone()])
            }
        }
        _ => return vec![],
    };
    el.attr("sqref")
        .split_whitespace()
        .filter_map(|s| {
            Some(Rule {
                sheet: id,
                area: parse_range(s).ok()?,
                kind: kind.into(),
                args: args.clone(),
            })
        })
        .collect()
}
fn filter(el: &Element, id: u32) -> Option<Filter> {
    let a = parse_range(el.attr("ref")).ok()?;
    let f = el.child("filterColumn")?;
    let col = a.c + f.attr("colId").parse::<i32>().ok()?;
    let condition = if let Some(values) = f.child("filters") {
        if values.attr("blank") == "1" && values.children.is_empty() {
            "vazio".into()
        } else {
            values
                .children
                .iter()
                .map(|v| v.attr("val"))
                .collect::<Vec<_>>()
                .join("|")
        }
    } else {
        let f = f.child("customFilters")?.child("customFilter")?;
        let val = f.attr("val");
        let sym = match f.attr("operator") {
            "" | "equal" if val.starts_with('*') && val.ends_with('*') => "~",
            "" | "equal" => "=",
            "notEqual" => "<>",
            "lessThan" => "<",
            "lessThanOrEqual" => "<=",
            "greaterThan" => ">",
            "greaterThanOrEqual" => ">=",
            _ => return None,
        };
        format!(
            "{sym}{}",
            if sym == "~" {
                val.trim_matches('*')
            } else {
                val
            }
        )
    };
    Some(Filter {
        sheet: id,
        area: a,
        col,
        condition,
        hidden: Default::default(),
        excluded: Default::default(),
    })
}
pub(super) fn extras(files: &Parts, extra: &mut Extras) -> Result {
    let workbook = root(&xml(files, "xl/workbook.xml")?)?;
    let rels = relations(files, "xl/_rels/workbook.xml.rels", "xl")?;
    for sheet in &workbook.child("sheets").ok_or("Pasta sem abas")?.children {
        let id = sheet.attr("sheetId").parse::<u32>().map_err(err)?;
        let key = rels
            .get(sheet.attr("r:id"))
            .ok_or("Relacionamento de aba ausente")?;
        let text = xml(files, key)?;
        let mut r = Reader::from_str(&text);
        let mut cell = None;
        loop {
            match r.read_event().map_err(err)? {
                Event::Start(t) if t.local_name().as_ref() == b"c" => {
                    cell = t
                        .try_get_attribute("r")
                        .map_err(err)?
                        .and_then(|a| parse_addr(std::str::from_utf8(&a.value).ok()?).ok())
                }
                Event::Start(t) if t.local_name().as_ref() == b"f" => {
                    let body = r.read_text(t.name()).map_err(err)?;
                    if body.contains('[') {
                        if let Some((row, c)) = cell {
                            extra.expressions.push(Expression {
                                sheet: id,
                                r: row,
                                c,
                                text: format!(
                                    "={}",
                                    quick_xml::escape::unescape(&body).map_err(err)?
                                ),
                            })
                        }
                    }
                }
                Event::End(t) if t.local_name().as_ref() == b"c" => cell = None,
                Event::Start(t)
                    if t.local_name().as_ref() == b"dataValidations"
                        || t.local_name().as_ref() == b"autoFilter" =>
                {
                    let mut el = element(&t)?;
                    children(&mut el, &mut r)?;
                    if el.tag == "autoFilter" {
                        extra.filters.extend(filter(&el, id))
                    } else {
                        for c in el.children {
                            extra.rules.extend(rule(&c, id))
                        }
                    }
                }
                Event::Eof => break,
                _ => (),
            }
        }
    }
    for (k, data) in files
        .iter()
        .filter(|(k, _)| k.starts_with("xl/tables/") && k.ends_with(".xml"))
    {
        let el = root(std::str::from_utf8(data).map_err(err)?)?;
        let Some(t) = extra.tables.iter_mut().find(|t| t.name == el.attr("name")) else {
            continue;
        };
        if let Some(cols) = el.child("tableColumns") {
            for (j, col) in cols.children.iter().enumerate() {
                if let Some(calc) = col.child("calculatedColumnFormula") {
                    t.formulas.insert(
                        t.area.c + j as i32,
                        format!("={}", calc.text.trim_start_matches('=')),
                    );
                }
            }
        }
        if let Some(f) = el.child("autoFilter").and_then(|c| filter(c, t.sheet)) {
            if !extra.filters.iter().any(|x| x.sheet == t.sheet) {
                extra.filters.push(Filter { area: t.area, ..f })
            }
        }
        let _ = k;
    }
    Ok(())
}
