//! Metadata for features with no mutable IronCalc API. Sheet identity is stable across tab reordering.
use crate::book::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Extras {
    pub tables: Vec<Table>,
    pub rules: Vec<Rule>,
    pub filters: Vec<Filter>,
    pub expressions: Vec<Expression>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub sheet: u32,
    pub sheet_name: String,
    pub area: Range,
    pub headers: Vec<String>,
    pub formulas: BTreeMap<i32, String>,
    pub total: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Rule {
    pub sheet: u32,
    pub area: Range,
    pub kind: String,
    pub args: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Filter {
    pub sheet: u32,
    pub area: Range,
    pub col: i32,
    pub condition: String,
    pub hidden: BTreeSet<i32>,
    #[serde(default)]
    pub excluded: BTreeSet<i32>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Expression {
    pub sheet: u32,
    pub r: i32,
    pub c: i32,
    pub text: String,
}

impl Extras {
    pub fn region_managed(&self, s: u32, a: Range) -> bool {
        self.tables
            .iter()
            .any(|t| t.sheet == s && overlap(t.area, a))
            || self
                .rules
                .iter()
                .any(|v| v.sheet == s && overlap(v.area, a))
            || self
                .expressions
                .iter()
                .any(|e| e.sheet == s && a.contains(e.r, e.c))
    }
    pub fn original(&self, s: u32, r: i32, c: i32) -> Option<&str> {
        self.expressions
            .iter()
            .find(|e| (e.sheet, e.r, e.c) == (s, r, c))
            .map(|e| e.text.as_str())
    }
    pub fn record(&mut self, s: u32, r: i32, c: i32, v: &str, translated: &str) {
        self.expressions
            .retain(|e| (e.sheet, e.r, e.c) != (s, r, c));
        if v.starts_with('=') && v.contains('[') && v != translated {
            self.expressions.push(Expression {
                sheet: s,
                r,
                c,
                text: v.into(),
            })
        }
    }
    pub fn validate(&self, s: u32, r: i32, c: i32, v: &str) -> Result {
        if let Some(t) = self
            .tables
            .iter()
            .find(|t| t.sheet == s && r == t.area.r && t.area.contains(r, c))
        {
            if t.headers[(c - t.area.c) as usize] != v {
                return Err("Use renomear-coluna para alterar o cabeçalho da tabela".into());
            }
        }
        for rule in &self.rules {
            if rule.sheet != s || !rule.area.contains(r, c) || v.is_empty() {
                continue;
            }
            let num = |v: &str| v.replace(',', ".").parse::<f64>().ok();
            let ok = match rule.kind.as_str() {
                "lista" => rule.args.iter().any(|x| x == v),
                "numero" => num(v)
                    .zip(num(&rule.args[0]))
                    .zip(num(&rule.args[1]))
                    .is_some_and(|((n, lo), hi)| n >= lo && n <= hi),
                "data" => iso_date(v) && v >= rule.args[0].as_str() && v <= rule.args[1].as_str(),
                _ => false,
            };
            if !ok {
                return Err(format!(
                    "Valor fora da validação {} ({})",
                    rule.kind,
                    rule.args.join(", ")
                ));
            }
        }
        Ok(())
    }
    pub fn translate(&self, s: u32, r: i32, c: i32, text: &str) -> Result<String> {
        if !text.starts_with('=') || !text.contains('[') {
            return Ok(text.into());
        }
        map_structured(text, |name, h, row| {
            let t = if name.is_empty() {
                self.tables
                    .iter()
                    .find(|t| t.sheet == s && t.area.contains(r, c))
            } else {
                self.tables
                    .iter()
                    .find(|t| t.name.eq_ignore_ascii_case(name))
            };
            let Some(t) = t else { return Ok(None) };
            let index = t
                .headers
                .iter()
                .position(|v| v.eq_ignore_ascii_case(h))
                .ok_or_else(|| format!("Coluna desconhecida: {h}"))?;
            let col = ironcalc::base::expressions::utils::number_to_column(t.area.c + index as i32)
                .unwrap();
            let sheet = t.sheet_name.replace('\'', "''");
            Ok(Some(if row {
                format!("'{sheet}'!${col}{r}")
            } else {
                format!(
                    "'{sheet}'!${col}${}:${col}${}",
                    t.area.r + 1,
                    t.area.end - i32::from(t.total)
                )
            }))
        })
    }
}
// Recognize only the two supported forms, keeping quoted Excel strings and sheet identifiers opaque.
pub(crate) fn map_structured(
    text: &str,
    mut f: impl FnMut(&str, &str, bool) -> Result<Option<String>>,
) -> Result<String> {
    let mut out = String::new();
    let mut i = 0;
    while i < text.len() {
        let tail = &text[i..];
        let ch = tail.chars().next().unwrap();
        if ch == '"' || ch == '\'' {
            let mut j = i + ch.len_utf8();
            while j < text.len() {
                let c = text[j..].chars().next().unwrap();
                j += c.len_utf8();
                if c == ch {
                    if text[j..].starts_with(ch) {
                        j += ch.len_utf8()
                    } else {
                        break;
                    }
                }
            }
            out.push_str(&text[i..j]);
            i = j;
            continue;
        }
        let name_end = i + tail
            .find(|v: char| !v.is_alphanumeric() && v != '_')
            .unwrap_or(tail.len());
        if let Some(rest) = text[name_end..].strip_prefix("[[#This Row],[") {
            if let Some(k) = rest.find("]]") {
                if let Some(v) = f(&text[i..name_end], &rest[..k], true)? {
                    out.push_str(&v);
                    i = text.len() - rest.len() + k + 2;
                    continue;
                }
            }
        }
        if text[name_end..].starts_with('[') {
            if let Some(k) = text[name_end + 1..].find(']') {
                let end = name_end + 1 + k;
                let h = &text[name_end + 1..end];
                let row = h.starts_with('@');
                if !h.contains(['[', '#']) {
                    if let Some(v) = f(&text[i..name_end], h.trim_start_matches('@'), row)? {
                        out.push_str(&v);
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        if name_end > i {
            out.push_str(&text[i..name_end]);
            i = name_end
        } else {
            out.push(ch);
            i += ch.len_utf8()
        }
    }
    Ok(out)
}
pub fn overlap(a: Range, b: Range) -> bool {
    a.r <= b.end && a.end >= b.r && a.c <= b.right && a.right >= b.c
}
pub fn iso_date(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() && s.len() == 10
}
impl Book {
    pub fn create_table(&mut self, s: u32, a: Range, name: &str) -> Result {
        a.bounded()?;
        if a.r == a.end {
            return Err("Inclua cabeçalho e dados".into());
        }
        if !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
            || parse_addr(name).is_ok()
            || self
                .extra
                .tables
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(name))
            || self
                .m
                .get_defined_name_list()
                .iter()
                .any(|(n, _, _)| n.eq_ignore_ascii_case(name))
        {
            return Err("Nome de tabela inválido ou em uso".into());
        }
        let id = self.ws(s).sheet_id;
        if self
            .extra
            .tables
            .iter()
            .any(|t| t.sheet == id && overlap(a, t.area))
        {
            return Err("Tabelas sobrepostas".into());
        }
        let mut headers = Vec::<String>::new();
        for c in a.c..=a.right {
            let h = self.val(s, a.r, c);
            if h.is_empty()
                || h.contains(['[', ']', '#', '@'])
                || headers.iter().any(|v| v.eq_ignore_ascii_case(&h))
            {
                return Err("Use cabeçalhos de texto distintos, sem colchetes".into());
            }
            headers.push(h)
        }
        self.extra.tables.push(Table {
            sheet: id,
            sheet_name: self.ws(s).name.clone(),
            name: name.into(),
            area: a,
            headers,
            formulas: BTreeMap::new(),
            total: false,
        });
        Ok(())
    }
    pub fn convert_table(&mut self, sheet: u32, row: i32, col: i32) -> Result {
        let index = self.table_at(sheet, row, col)?;
        let table = self.extra.tables[index].clone();
        let sheets = self.m.get_worksheets_properties();
        let mut updates = Vec::new();
        for expr in &self.extra.expressions {
            let local = expr.sheet == table.sheet && table.area.contains(expr.r, expr.c);
            let text = map_structured(&expr.text, |name, header, this_row| {
                if !(name.eq_ignore_ascii_case(&table.name) || name.is_empty() && local) {
                    return Ok(None);
                }
                let operand = format!("={name}[{}{header}]", if this_row { "@" } else { "" });
                let lowered = self.extra.translate(expr.sheet, expr.r, expr.c, &operand)?;
                Ok(Some(lowered[1..].into()))
            })?;
            if text != expr.text {
                let s = sheets
                    .iter()
                    .position(|w| w.sheet_id == expr.sheet)
                    .ok_or("Aba ausente")?;
                updates.push((s as u32, expr.r, expr.c, text));
            }
        }
        if self
            .extra
            .filters
            .iter()
            .any(|f| f.sheet == table.sheet && f.area == table.area)
        {
            self.unfilter(sheet)?;
        }
        self.extra.tables.remove(index);
        for (s, r, c, text) in updates {
            self.set(s, r, c, &text)?
        }
        for i in 0..self.extra.tables.len() {
            let t = &self.extra.tables[i];
            let s = sheets
                .iter()
                .position(|w| w.sheet_id == t.sheet)
                .ok_or("Aba ausente")?;
            let templates = t
                .formulas
                .keys()
                .map(|&c| (c, self.raw(s as u32, t.area.r + 1, c)))
                .collect();
            self.extra.tables[i].formulas = templates;
        }
        Ok(())
    }
    pub fn table_at(&self, s: u32, r: i32, c: i32) -> Result<usize> {
        self.extra
            .tables
            .iter()
            .position(|t| t.sheet == self.ws(s).sheet_id && t.area.contains(r, c))
            .ok_or("Selecione uma célula da tabela".into())
    }
    pub fn refresh_refs(&mut self) -> Result {
        for e in self.extra.expressions.clone() {
            if let Some(s) = self
                .m
                .get_worksheets_properties()
                .iter()
                .position(|x| x.sheet_id == e.sheet)
            {
                self.set(s as u32, e.r, e.c, &e.text)?
            }
        }
        Ok(())
    }
    pub fn table_formula(&mut self, s: u32, r: i32, c: i32, v: &str) -> Result {
        if !v.starts_with('=') {
            return Err("Use uma fórmula iniciada por =".into());
        }
        let i = self.table_at(s, r, c)?;
        let t = self.extra.tables[i].clone();
        let mut model = ironcalc::base::Model::from_bytes(&self.m.to_bytes(), "en")?;
        use ironcalc::base::expressions::types::CellReferenceIndex as Ref;
        let anchor = Ref {
            sheet: s,
            row: r.clamp(t.area.r + 1, t.area.end - i32::from(t.total)),
            column: c,
        };
        for rr in t.area.r + 1..=t.area.end - i32::from(t.total) {
            let f = crate::structure::copied(&mut model, v, anchor, Ref { row: rr, ..anchor })?;
            self.set(s, rr, c, &f)?;
        }
        let first = self.raw(s, t.area.r + 1, c);
        self.extra.tables[i].formulas.insert(c, first);
        Ok(())
    }
    pub fn extend_table(&mut self, s: u32, r: i32, c: i32, totals: bool) -> Result {
        let i = self.table_at(s, r, c)?;
        let t = self.extra.tables[i].clone();
        if t.total {
            return Err("Remova a linha de totais antes de acrescentar registros".into());
        }
        let last = t.area.end + 1;
        if last > ROWS || (t.area.c..=t.area.right).any(|c| !self.raw(s, last, c).is_empty()) {
            return Err("A linha seguinte deve estar vazia".into());
        }
        let region = Range {
            r: last,
            end: last,
            ..t.area
        };
        if self
            .extra
            .tables
            .iter()
            .enumerate()
            .any(|(n, o)| n != i && o.sheet == t.sheet && overlap(o.area, region))
        {
            return Err("Outra tabela ocupa a linha seguinte".into());
        }
        if let Some(filter) = self
            .extra
            .filters
            .iter_mut()
            .find(|f| f.sheet == t.sheet && f.area == t.area)
        {
            filter.area.end = last;
        }
        self.extra.tables[i].area.end = last;
        self.extra.tables[i].total = totals;
        if totals {
            self.set(s, last, t.area.c, "Total")?;
            for col in t.area.c + 1..=t.area.right {
                if (t.area.r + 1..=t.area.end)
                    .any(|r| self.number(s, r, col).is_some() || t.formulas.contains_key(&col))
                {
                    let sep = if self.m.get_locale() == "pt" {
                        ';'
                    } else {
                        ','
                    };
                    self.set(
                        s,
                        last,
                        col,
                        &format!(
                            "=SUBTOTAL(109{sep}{}[{}])",
                            t.name,
                            t.headers[(col - t.area.c) as usize]
                        ),
                    )?
                }
            }
        } else {
            let mut model = ironcalc::base::Model::from_bytes(&self.m.to_bytes(), "en")?;
            for (col, v) in t.formulas {
                use ironcalc::base::expressions::types::CellReferenceIndex as Ref;
                let source = Ref {
                    sheet: s,
                    row: t.area.r + 1,
                    column: col,
                };
                self.set(
                    s,
                    last,
                    col,
                    &crate::structure::copied(
                        &mut model,
                        &v,
                        source,
                        Ref {
                            row: last,
                            ..source
                        },
                    )?,
                )?
            }
        }
        self.refresh_refs()
    }
    pub fn rename_table_part(&mut self, s: u32, r: i32, c: i32, v: &str, column: bool) -> Result {
        let i = self.table_at(s, r, c)?;
        let t = self.extra.tables[i].clone();
        if v.is_empty() || v.contains(['[', ']', '#', '@']) {
            return Err("Nome inválido".into());
        }
        let old = &t.headers[(c - t.area.c) as usize];
        if column {
            if t.headers.iter().any(|h| h.eq_ignore_ascii_case(v)) {
                return Err("Cabeçalho repetido".into());
            }
            self.extra.tables[i].headers[(c - t.area.c) as usize] = v.into();
            self.set(s, t.area.r, c, v)?
        } else {
            if !v.starts_with(|c: char| c.is_alphabetic() || c == '_')
                || !v.chars().all(|c| c.is_alphanumeric() || c == '_')
                || parse_addr(v).is_ok()
                || self
                    .extra
                    .tables
                    .iter()
                    .any(|t| t.name.eq_ignore_ascii_case(v))
            {
                return Err("Nome inválido ou repetido".into());
            }
            self.extra.tables[i].name = v.into()
        }
        let rename = |text: &str, local: bool| {
            map_structured(text, |name, h, row| {
                if !(name.eq_ignore_ascii_case(&t.name) || (name.is_empty() && local)) {
                    return Ok(None);
                }
                let name = if !column && !name.is_empty() { v } else { name };
                let h = if column && h.eq_ignore_ascii_case(old) {
                    v
                } else {
                    h
                };
                Ok(Some(format!("{name}[{}{h}]", if row { "@" } else { "" })))
            })
        };
        for e in &mut self.extra.expressions {
            e.text = rename(&e.text, e.sheet == t.sheet && t.area.contains(e.r, e.c))?
        }
        for (j, tab) in self.extra.tables.iter_mut().enumerate() {
            for f in tab.formulas.values_mut() {
                *f = rename(f, j == i)?
            }
        }
        self.refresh_refs()
    }
    pub fn unfilter(&mut self, s: u32) -> Result {
        let id = self.ws(s).sheet_id;
        if let Some(i) = self.extra.filters.iter().position(|f| f.sheet == id) {
            let f = self.extra.filters.remove(i);
            for r in f.hidden {
                self.m.set_rows_hidden(s, r, r, false)?
            }
        }
        Ok(())
    }
    pub(crate) fn reapply_filters(&mut self) -> Result {
        if self.extra.filters.is_empty() {
            return Ok(());
        }
        self.m.evaluate();
        let sheets = self.m.get_worksheets_properties();
        let filters: Vec<_> = self
            .extra
            .filters
            .iter()
            .map(|f| (f.sheet, f.area, f.col, f.condition.clone()))
            .collect();
        for (id, area, col, condition) in filters {
            let s = sheets
                .iter()
                .position(|w| w.sheet_id == id)
                .ok_or("Aba de filtro inválida")?;
            self.filter(s as u32, area, col, &condition)?;
        }
        Ok(())
    }
    pub fn last_filter_record(&self, s: u32, a: Range) -> i32 {
        a.end
            - i32::from(
                self.extra
                    .tables
                    .iter()
                    .any(|t| t.sheet == self.ws(s).sheet_id && t.area == a && t.total),
            )
    }
    pub fn filter(&mut self, s: u32, a: Range, col: i32, condition: &str) -> Result {
        a.bounded()?;
        if !(a.c..=a.right).contains(&col) || a.end == a.r {
            return Err("Inclua cabeçalho e coluna do filtro".into());
        }
        self.unfilter(s)?;
        let mut hidden = BTreeSet::new();
        let mut excluded = BTreeSet::new();
        for r in a.r + 1..=self.last_filter_record(s, a) {
            let v = self.val(s, r, col);
            let ok = matches_condition(&v, self.number(s, r, col), condition);
            if !ok {
                excluded.insert(r);
            }
            if !ok && !self.m.get_model().is_row_hidden(s, r)? {
                self.m.set_rows_hidden(s, r, r, true)?;
                hidden.insert(r);
            }
        }
        self.extra.filters.push(Filter {
            sheet: self.ws(s).sheet_id,
            area: a,
            col,
            condition: condition.into(),
            hidden,
            excluded,
        });
        Ok(())
    }
    pub fn sort_rows(&mut self, s: u32, a: Range, keys: &[(i32, bool)], header: bool) -> Result {
        if let Some(i) = self
            .extra
            .tables
            .iter()
            .position(|t| t.sheet == self.ws(s).sheet_id && t.area == a)
        {
            return self.sort_table(s, i, keys);
        }
        self.sort_impl(s, a, keys, header, true).map(|_| ())
    }
    fn sort_impl(
        &mut self,
        s: u32,
        a: Range,
        keys: &[(i32, bool)],
        header: bool,
        check_edges: bool,
    ) -> Result<Vec<i32>> {
        a.bounded()?;
        if keys.is_empty() || keys.iter().any(|(c, _)| !(a.c..=a.right).contains(c)) {
            return Err("Chave fora da seleção".into());
        }
        if self.extra.region_managed(self.ws(s).sheet_id, a)
            || !self.m.get_merged_cells(s)?.is_empty()
        {
            return Err("Ordenação requer intervalo simples sem mesclas".into());
        }
        let mut order: Vec<_> = (a.r + i32::from(header)..=a.end).collect();
        // Refuse a partial record unless the user expands the selection.
        if check_edges
            && order.iter().any(|r| {
                (a.c > 1 && !self.raw(s, *r, a.c - 1).is_empty())
                    || (a.right < COLS && !self.raw(s, *r, a.right + 1).is_empty())
            })
        {
            return Err("Amplie a seleção para incluir o registro completo".into());
        }
        order.sort_by(|x, y| {
            for &(c, desc) in keys {
                let key = |r| {
                    let v = self.val(s, r, c);
                    let n = self.number(s, r, c);
                    let rank = if v.is_empty() {
                        3
                    } else if v.starts_with('#') {
                        2
                    } else if n.is_some() {
                        0
                    } else {
                        1
                    };
                    (rank, n, v)
                };
                let (ax, nx, vx) = key(*x);
                let (ay, ny, vy) = key(*y);
                let o = ax.cmp(&ay).then_with(|| match (nx, ny) {
                    (Some(x), Some(y)) => x.total_cmp(&y),
                    _ => vx.cmp(&vy),
                });
                if !o.is_eq() {
                    return if desc { o.reverse() } else { o };
                }
            }
            std::cmp::Ordering::Equal
        });
        let clips = order
            .iter()
            .map(|r| {
                self.clipboard(
                    s,
                    Range {
                        r: *r,
                        end: *r,
                        ..a
                    },
                    false,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        for (i, clip) in clips.iter().enumerate() {
            self.paste(clip, s, a.r + i32::from(header) + i as i32, a.c, "normal")?
        }
        Ok(order)
    }
}
pub(crate) fn matches_condition(v: &str, num: Option<f64>, expr: &str) -> bool {
    if expr == "vazio" {
        return v.is_empty();
    }
    if let Some(s) = expr.strip_prefix('~') {
        return v.to_lowercase().contains(&s.to_lowercase());
    }
    for op in [">=", "<=", "<>", ">", "<", "="] {
        if let Some(rhs) = expr.strip_prefix(op) {
            let cmp = if let (Some(x), Some(y)) = (num, criterion_number(rhs)) {
                x.total_cmp(&y)
            } else {
                v.cmp(rhs)
            };
            return match op {
                ">=" => !cmp.is_lt(),
                "<=" => !cmp.is_gt(),
                "<>" => !cmp.is_eq(),
                ">" => cmp.is_gt(),
                "<" => cmp.is_lt(),
                _ => cmp.is_eq(),
            };
        }
    }
    expr.split('|').any(|x| x == v)
}
impl Book {
    /// Change the table boundary while keeping cells outside it as ordinary cells.
    pub fn resize_table(&mut self, s: u32, r: i32, c: i32, new: Range) -> Result {
        new.bounded()?;
        let i = self.table_at(s, r, c)?;
        let t = self.extra.tables[i].clone();
        if new.r != t.area.r || new.c != t.area.c || new.end <= new.r {
            return Err("Mantenha a origem e inclua pelo menos um registro".into());
        }
        if t.total {
            return Err("Desative os totais antes de redimensionar".into());
        }
        if self
            .extra
            .tables
            .iter()
            .enumerate()
            .any(|(n, o)| n != i && o.sheet == t.sheet && overlap(new, o.area))
        {
            return Err("Sobreposição entre tabelas".into());
        }
        let mut headers = Vec::<String>::new();
        for cc in new.c..=new.right {
            let h = self.val(s, new.r, cc);
            if h.is_empty()
                || headers.iter().any(|v| v.eq_ignore_ascii_case(&h))
                || h.contains(['[', ']', '#', '@'])
            {
                return Err("Cabeçalho vazio, inválido ou duplicado".into());
            }
            headers.push(h)
        }
        // Preserve current formulas in cells released by a shrink as ordinary A1 formulas.
        self.extra.expressions.retain(|e| {
            !(e.sheet == t.sheet && t.area.contains(e.r, e.c) && !new.contains(e.r, e.c))
        });
        self.extra.tables[i].headers = headers;
        if let Some(filter) = self
            .extra
            .filters
            .iter_mut()
            .find(|f| f.sheet == t.sheet && f.area == t.area)
        {
            if filter.col > new.right {
                self.unfilter(s)?;
            } else {
                filter.area = new;
            }
        }
        self.extra.tables[i].area = new;
        self.extra.tables[i]
            .formulas
            .retain(|cc, _| *cc <= new.right);
        for (col, f) in &t.formulas {
            if *col <= new.right {
                for rr in t.area.end + 1..=new.end {
                    if !self.raw(s, rr, *col).is_empty() {
                        return Err("Expansão sobrescreveria dados na coluna calculada".into());
                    }
                    self.set(s, rr, *col, f)?
                }
            }
        }
        self.refresh_refs()
    }
    pub fn toggle_totals(&mut self, s: u32, r: i32, c: i32) -> Result {
        let i = self.table_at(s, r, c)?;
        let t = self.extra.tables[i].clone();
        if !t.total {
            return self.extend_table(s, r, c, true);
        }
        for cc in t.area.c..=t.area.right {
            self.set(s, t.area.end, cc, "")?
        }
        if let Some(filter) = self
            .extra
            .filters
            .iter_mut()
            .find(|f| f.sheet == t.sheet && f.area == t.area)
        {
            filter.area.end -= 1;
        }
        self.extra.tables[i].total = false;
        self.extra.tables[i].area.end -= 1;
        self.refresh_refs()
    }
    pub fn sort_table(&mut self, s: u32, i: usize, keys: &[(i32, bool)]) -> Result {
        let t = self.extra.tables[i].clone();
        let f = self
            .extra
            .filters
            .iter()
            .find(|f| f.sheet == t.sheet)
            .cloned();
        self.unfilter(s)?;
        let a = Range {
            end: t.area.end - i32::from(t.total),
            ..t.area
        };
        // Records travel with their native styles and A1 references. Capture
        // structured formulas by record, then restore their binding in the new row.
        let mut extra = std::mem::take(&mut self.extra);
        let order = self.sort_impl(s, a, keys, true, false)?;
        let prior = extra.expressions.clone();
        extra
            .expressions
            .retain(|e| e.sheet != t.sheet || e.r <= a.r || !a.contains(e.r, e.c));
        let mut model = None;
        for (offset, source) in order.into_iter().enumerate() {
            let dest = a.r + 1 + offset as i32;
            for mut e in prior
                .iter()
                .filter(|e| e.sheet == t.sheet && e.r == source && a.contains(e.r, e.c))
                .cloned()
            {
                use ironcalc::base::expressions::types::CellReferenceIndex as Ref;
                if model.is_none() {
                    model = Some(ironcalc::base::Model::from_bytes(&self.m.to_bytes(), "en")?);
                }
                let origin = Ref {
                    sheet: s,
                    row: source,
                    column: e.c,
                };
                e.text = crate::structure::copied(
                    model.as_mut().unwrap(),
                    &e.text,
                    origin,
                    Ref {
                        row: dest,
                        ..origin
                    },
                )?;
                e.r = dest;
                extra.expressions.push(e)
            }
        }
        self.extra = extra;
        self.refresh_refs()?;
        self.m.evaluate();
        if let Some(f) = f {
            self.filter(s, f.area, f.col, &f.condition)?
        }
        Ok(())
    }
}

fn criterion_number(v: &str) -> Option<f64> {
    use chrono::Datelike;
    if iso_date(v) {
        let d = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok()?;
        return ironcalc::base::formatter::dates::date_to_serial_number(
            d.day(),
            d.month(),
            d.year(),
        )
        .ok()
        .map(f64::from);
    }
    v.replace(',', ".").parse().ok()
}
