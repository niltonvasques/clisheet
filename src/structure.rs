//! Apply IronCalc's reference transformations to the ordinary references in
//! mixed table/A1 expressions. A structured token occupies one AST operand.
use crate::{
    book::*,
    data::{map_structured, Expression},
};
use ironcalc::base::{expressions::types::CellReferenceIndex as Ref, Model};
pub(crate) struct Mask {
    prefix: String,
    refs: Vec<String>,
    pub text: String,
}
impl Mask {
    pub fn new(value: &str) -> Result<Self> {
        let mut prefix = "__clisheet_ref".to_string();
        while value.contains(&prefix) {
            prefix.push('_')
        }
        let mut refs = Vec::new();
        let text = map_structured(value, |n, h, row| {
            let i = refs.len();
            refs.push(format!("{n}[{}{h}]", if row { "@" } else { "" }));
            Ok(Some(format!("\"{prefix}{i}\"")))
        })?;
        Ok(Self { prefix, refs, text })
    }
    pub fn restore(&self, text: &str) -> String {
        // Every marker is a complete Excel string literal. The prefix was chosen
        // absent from all input, so it cannot match another operand or sheet name.
        self.refs
            .iter()
            .enumerate()
            .fold(text.into(), |v, (i, label)| {
                v.replace(&format!("\"{}{i}\"", self.prefix), label)
            })
    }
}
pub fn copied(m: &mut Model, formula: &str, source: Ref, target: Ref) -> Result<String> {
    let mask = Mask::new(formula)?;
    Ok(mask.restore(&m.extend_copied_value(&mask.text, &source, &target)?))
}
#[derive(Clone, Copy)]
struct Shift {
    pos: i32,
    n: i32,
    remove: bool,
    rows: bool,
}
impl Shift {
    fn point(self, p: i32) -> Option<i32> {
        if p < self.pos {
            Some(p)
        } else if !self.remove {
            Some(p + self.n)
        } else if p >= self.pos + self.n {
            Some(p - self.n)
        } else {
            None
        }
    }
    fn bounds(self, lo: i32, hi: i32) -> Option<(i32, i32)> {
        if !self.remove {
            return Some((self.point(lo)?, self.point(hi)?));
        }
        let first = if lo >= self.pos && lo < self.pos + self.n {
            self.pos + self.n
        } else {
            lo
        };
        let last = if hi >= self.pos && hi < self.pos + self.n {
            self.pos - 1
        } else {
            hi
        };
        (first <= last).then(|| (self.point(first).unwrap(), self.point(last).unwrap()))
    }
    fn range(self, mut a: Range) -> Option<Range> {
        if self.rows {
            (a.r, a.end) = self.bounds(a.r, a.end)?
        } else {
            (a.c, a.right) = self.bounds(a.c, a.right)?
        }
        Some(a)
    }
    fn cell(self, r: i32, c: i32) -> Option<(i32, i32)> {
        if self.rows {
            Some((self.point(r)?, c))
        } else {
            Some((r, self.point(c)?))
        }
    }
}
impl Book {
    pub fn shift(&mut self, s: u32, a: Range, rows: bool, remove: bool) -> Result {
        let id = self.ws(s).sheet_id;
        let area = a.area(s);
        let edit = Shift {
            pos: if rows { a.r } else { a.c },
            n: if rows { area.height } else { area.width },
            rows,
            remove,
        };
        let filter = self.extra.filters.iter().find(|f| f.sheet == id).cloned();
        self.unfilter(s)?;
        let mut masked = Vec::new();
        for e in std::mem::take(&mut self.extra.expressions) {
            let idx = self
                .m
                .get_worksheets_properties()
                .iter()
                .position(|w| w.sheet_id == e.sheet)
                .ok_or("Aba inválida")? as u32;
            let mask = Mask::new(&e.text)?;
            self.m.set_user_input(idx, e.r, e.c, &mask.text)?;
            if let Some((r, c)) = if e.sheet == id {
                edit.cell(e.r, e.c)
            } else {
                Some((e.r, e.c))
            } {
                masked.push((idx, r, c, mask))
            }
        }
        match (rows, remove) {
            (true, true) => self.m.delete_rows(s, edit.pos, edit.n)?,
            (true, false) => self.m.insert_rows(s, edit.pos, edit.n)?,
            (false, true) => self.m.delete_columns(s, edit.pos, edit.n)?,
            (false, false) => self.m.insert_columns(s, edit.pos, edit.n)?,
        };
        let notes = self.m.notes_mut(s)?;
        for mut note in std::mem::take(notes) {
            let (r, c) = parse_addr(&note.cell_ref)?;
            if let Some((r, c)) = edit.cell(r, c) {
                Range::cell(r, c).bounded()?;
                note.cell_ref = addr(r, c);
                notes.push(note);
            }
        }
        self.extra.rules.retain_mut(|rule| {
            if rule.sheet != id {
                return true;
            }
            if let Some(a) = edit.range(rule.area) {
                rule.area = a;
                true
            } else {
                false
            }
        });
        let mut new_cells = Vec::new();
        for mut table in std::mem::take(&mut self.extra.tables) {
            if table.sheet != id {
                self.extra.tables.push(table);
                continue;
            }
            let old = table.area;
            let Some(next) = edit.range(old) else {
                continue;
            };
            next.bounded()?;
            if rows && remove && edit.point(old.r).is_none() {
                return Err("Converta a tabela antes de excluir seu cabeçalho".into());
            }
            if next.r == next.end {
                return Err("A tabela precisa de um registro".into());
            }
            if rows && remove && table.total && edit.point(old.end).is_none() {
                table.total = false
            }
            if !rows {
                let mut headers = vec![String::new(); (next.right - next.c + 1) as usize];
                for (j, h) in table.headers.iter().enumerate() {
                    if let Some(col) = edit.point(old.c + j as i32) {
                        headers[(col - next.c) as usize] = h.clone()
                    }
                }
                table.formulas = table
                    .formulas
                    .into_iter()
                    .filter_map(|(c, v)| Some((edit.point(c)?, v)))
                    .collect();
                for col in next.c..=next.right {
                    let j = (col - next.c) as usize;
                    if headers[j].is_empty() {
                        let mut n = 1;
                        while headers
                            .iter()
                            .any(|h| h.eq_ignore_ascii_case(&format!("Coluna{n}")))
                        {
                            n += 1
                        }
                        headers[j] = format!("Coluna{n}");
                        self.m.set_user_input(s, next.r, col, &headers[j])?
                    }
                }
                table.headers = headers;
            } else if !remove && edit.pos > old.r && edit.pos <= old.end {
                for rr in edit.pos..edit.pos + edit.n {
                    for (&cc, f) in &table.formulas {
                        new_cells.push((rr, cc, f.clone()))
                    }
                }
            }
            table.area = next;
            self.extra.tables.push(table);
        }
        for (idx, r, c, mask) in masked {
            let raw = self.m.get_cell_content(idx, r, c)?;
            let value = mask.restore(&raw);
            if let Err(err) = self.set(idx, r, c, &value) {
                if err.starts_with("Coluna desconhecida") {
                    let v = map_structured(&value, |name, h, _| {
                        let tab = if name.is_empty() {
                            self.extra
                                .tables
                                .iter()
                                .find(|t| t.sheet == self.ws(idx).sheet_id && t.area.contains(r, c))
                        } else {
                            self.extra
                                .tables
                                .iter()
                                .find(|t| t.name.eq_ignore_ascii_case(name))
                        };
                        Ok((!tab.is_some_and(|t| {
                            t.headers.iter().any(|col| col.eq_ignore_ascii_case(h))
                        }))
                        .then(|| "#REF!".into()))
                    })?;
                    self.set(idx, r, c, &v)?
                } else {
                    return Err(err);
                }
            }
        }
        for (r, c, v) in new_cells {
            self.set(s, r, c, &v)?
        }
        // Sync the calculated-column template with the first remaining data row.
        for i in 0..self.extra.tables.len() {
            let t = self.extra.tables[i].clone();
            if t.sheet == id {
                for c in t.formulas.keys() {
                    let v = self.raw(s, t.area.r + 1, *c);
                    self.extra.tables[i].formulas.insert(*c, v);
                }
            }
        }
        if let Some(f) = filter {
            if let Some(a) = edit.range(f.area) {
                if let Some(col) = if rows { Some(f.col) } else { edit.point(f.col) } {
                    if a.end > a.r {
                        self.m.evaluate();
                        self.filter(s, a, col, &f.condition)?
                    }
                }
            }
        }
        Ok(())
    }
    pub fn duplicate_tab(&mut self, s: u32) -> Result {
        let old = self.ws(s).sheet_id;
        self.m.duplicate_sheet(s)?;
        let idx = self.m.get_selected_sheet();
        let new = self.ws(idx).sheet_id;
        let title = self.ws(idx).name.clone();
        let mut names = Vec::new();
        for mut t in self
            .extra
            .tables
            .clone()
            .into_iter()
            .filter(|t| t.sheet == old)
        {
            let old_name = t.name.clone();
            let mut n = 2;
            while self
                .extra
                .tables
                .iter()
                .any(|v| v.name.eq_ignore_ascii_case(&format!("{old_name}_{n}")))
            {
                n += 1
            }
            t.name = format!("{old_name}_{n}");
            names.push((old_name, t.name.clone()));
            t.sheet = new;
            t.sheet_name = title.clone();
            self.extra.tables.push(t);
        }
        let rename = |v: &str| {
            map_structured(v, |name, h, row| {
                Ok(names
                    .iter()
                    .find(|(o, _)| o.eq_ignore_ascii_case(name))
                    .map(|(_, n)| format!("{n}[{}{h}]", if row { "@" } else { "" })))
            })
        };
        for t in self.extra.tables.iter_mut().filter(|t| t.sheet == new) {
            for v in t.formulas.values_mut() {
                *v = rename(v)?
            }
        }
        for e in self
            .extra
            .expressions
            .clone()
            .into_iter()
            .filter(|e| e.sheet == old)
        {
            self.set(idx, e.r, e.c, &rename(&e.text)?)?
        }
        for mut rule in self
            .extra
            .rules
            .clone()
            .into_iter()
            .filter(|r| r.sheet == old)
        {
            rule.sheet = new;
            self.extra.rules.push(rule)
        }
        for mut f in self
            .extra
            .filters
            .clone()
            .into_iter()
            .filter(|f| f.sheet == old)
        {
            f.sheet = new;
            self.extra.filters.push(f)
        }
        Ok(())
    }
    /// Let the engine transform A1 operands while table operands remain symbolic.
    fn mask_sheet_expressions(&mut self) -> Result<Vec<(Expression, Mask)>> {
        let sheets = self.m.get_worksheets_properties();
        let mut masks = Vec::new();
        for e in std::mem::take(&mut self.extra.expressions) {
            let s = sheets
                .iter()
                .position(|w| w.sheet_id == e.sheet)
                .ok_or("Aba ausente")? as u32;
            let mask = Mask::new(&e.text)?;
            self.m.set_user_input(s, e.r, e.c, &mask.text)?;
            masks.push((e, mask));
        }
        Ok(masks)
    }

    fn restore_sheet_expressions(&mut self, masks: Vec<(Expression, Mask)>) -> Result {
        let sheets = self.m.get_worksheets_properties();
        for (e, mask) in masks {
            if let Some(s) = sheets.iter().position(|w| w.sheet_id == e.sheet) {
                let raw = self.m.get_cell_content(s as u32, e.r, e.c)?;
                self.set(s as u32, e.r, e.c, &mask.restore(&raw))?;
            }
        }
        for i in 0..self.extra.tables.len() {
            let table = &self.extra.tables[i];
            let s = sheets
                .iter()
                .position(|w| w.sheet_id == table.sheet)
                .ok_or("Aba ausente")?;
            let formulas = table
                .formulas
                .keys()
                .map(|&c| (c, self.raw(s as u32, table.area.r + 1, c)))
                .collect();
            self.extra.tables[i].formulas = formulas;
        }
        Ok(())
    }

    /// Move the actual cells through the engine with table operands masked.
    /// Applying the same move to the symbol locations also handles overlaps.
    pub(crate) fn cut_with_table_references(
        &mut self,
        src: u32,
        source: Range,
        dst: u32,
        target: Range,
    ) -> Result {
        let from_id = self.ws(src).sheet_id;
        let to_id = self.ws(dst).sheet_id;
        let masks = self.mask_sheet_expressions()?;
        let clip = self.clipboard(src, source, true)?;
        self.paste(&clip, dst, target.r, target.c, "normal")?;
        let relocated = masks
            .into_iter()
            .filter_map(|(mut expr, mask)| {
                if expr.sheet == from_id && source.contains(expr.r, expr.c) {
                    expr.sheet = to_id;
                    expr.r += target.r - source.r;
                    expr.c += target.c - source.c;
                } else if expr.sheet == to_id && target.contains(expr.r, expr.c) {
                    return None;
                }
                Some((expr, mask))
            })
            .collect();
        self.restore_sheet_expressions(relocated)
    }
    pub fn rename_tab(&mut self, s: u32, name: &str) -> Result {
        let masks = self.mask_sheet_expressions()?;
        let id = self.ws(s).sheet_id;
        self.m.rename_sheet(s, name)?;
        for t in self.extra.tables.iter_mut().filter(|t| t.sheet == id) {
            t.sheet_name = name.into()
        }
        self.restore_sheet_expressions(masks)
    }
    pub fn delete_tab(&mut self, s: u32) -> Result {
        let id = self.ws(s).sheet_id;
        let names: Vec<_> = self
            .extra
            .tables
            .iter()
            .filter(|t| t.sheet == id)
            .map(|t| t.name.clone())
            .collect();
        self.extra.tables.retain(|t| t.sheet != id);
        self.extra.filters.retain(|f| f.sheet != id);
        self.extra.rules.retain(|v| v.sheet != id);
        self.extra.expressions.retain(|e| e.sheet != id);
        for Expression { text, .. } in &mut self.extra.expressions {
            *text = map_structured(text, |n, _, _| {
                Ok(names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(n))
                    .then(|| "#REF!".into()))
            })?
        }
        let masks = self.mask_sheet_expressions()?;
        for (name, scope, _) in self.m.get_defined_name_list() {
            if scope == Some(s) {
                self.m.delete_defined_name(&name, scope)?
            }
        }
        self.m.delete_sheet(s)?;
        self.restore_sheet_expressions(masks)
    }
}
/// Translate canonical XLSX formula grammar using parsed operands. Structured
/// references remain symbolic for Excel, even though IronCalc lowers them to A1.
pub fn dialect(
    book: &Book,
    s: u32,
    r: i32,
    c: i32,
    formula: &str,
    from: &str,
    to: &str,
) -> Result<String> {
    use ironcalc::base::{
        expressions::{
            parser::{
                stringify::{to_excel_string, to_localized_string},
                Node, Parser,
            },
            types::CellReferenceRC,
        },
        language::get_default_language,
        locale::get_locale,
    };
    let mask = Mask::new(formula)?;
    let wb = &book.m.get_model().workbook;
    let ctx = CellReferenceRC {
        sheet: book.ws(s).name.clone(),
        row: r,
        column: c,
    };
    let mut parser = Parser::new(
        wb.worksheets.iter().map(|w| w.name.clone()).collect(),
        wb.get_defined_names_with_scope(),
        Default::default(),
        get_locale(from)?,
        get_default_language(),
    );
    let body = mask.text.strip_prefix('=').unwrap_or(&mask.text);
    let mut node = parser.parse(body, &ctx);
    if matches!(node, Node::ParseErrorKind { .. }) && from != "en" {
        parser.set_locale(get_locale("en")?);
        node = parser.parse(body, &ctx)
    }
    if matches!(node, Node::ParseErrorKind { .. }) {
        return Err("Fórmula estruturada incompatível com a sintaxe XLSX".into());
    }
    let text = if to == "xlsx" {
        to_excel_string(&node, &ctx)
    } else {
        to_localized_string(&node, &ctx, get_locale(to)?, get_default_language())
    };
    Ok(format!("={}", mask.restore(&text)))
}
