use crate::data::Extras;
use ironcalc::base::{
    cell::CellValue,
    expressions::{types::Area, utils::number_to_column},
    types::Worksheet,
    ClipboardData, UserModel,
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf};

pub type Result<T = ()> = std::result::Result<T, String>;
pub const ROWS: i32 = 1_048_576;
pub const COLS: i32 = 16_384;
pub const BATCH: i64 = 500_000;
pub const HISTORY: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub r: i32,
    pub c: i32,
    pub end: i32,
    pub right: i32,
}
impl Range {
    pub fn cell(r: i32, c: i32) -> Self {
        Self {
            r,
            c,
            end: r,
            right: c,
        }
    }
    pub fn between(a: (i32, i32), b: (i32, i32)) -> Self {
        Self {
            r: a.0.min(b.0),
            c: a.1.min(b.1),
            end: a.0.max(b.0),
            right: a.1.max(b.1),
        }
    }
    pub fn contains(self, r: i32, c: i32) -> bool {
        (self.r..=self.end).contains(&r) && (self.c..=self.right).contains(&c)
    }
    pub fn bounded(self) -> Result {
        if self.r < 1
            || self.c < 1
            || self.end > ROWS
            || self.right > COLS
            || self.r > self.end
            || self.c > self.right
        {
            Err("Intervalo fora da grade".into())
        } else if i64::from(self.end - self.r + 1) * i64::from(self.right - self.c + 1) > BATCH {
            Err("Selecione até 500 mil células por operação".into())
        } else {
            Ok(())
        }
    }
    pub fn area(self, sheet: u32) -> Area {
        Area {
            sheet,
            row: self.r,
            column: self.c,
            width: self.right - self.c + 1,
            height: self.end - self.r + 1,
        }
    }
    pub fn address(self) -> String {
        let a = addr(self.r, self.c);
        if self.r == self.end && self.c == self.right {
            a
        } else {
            format!("{}:{}", a, addr(self.end, self.right))
        }
    }
}
pub fn addr(r: i32, c: i32) -> String {
    format!("{}{r}", number_to_column(c).unwrap_or_default())
}
pub fn parse_addr(s: &str) -> Result<(i32, i32)> {
    let s = s.trim().replace('$', "").to_ascii_uppercase();
    let i = s
        .find(|c: char| !c.is_ascii_uppercase())
        .ok_or("Falta o número da linha")?;
    if i == 0 || i > 3 {
        return Err("Coluna inválida".into());
    }
    let col = s[..i]
        .bytes()
        .fold(0, |n, b| n * 26 + (b - b'A' + 1) as i32);
    let row = s[i..].parse::<i32>().map_err(|_| "Linha inválida")?;
    if !(1..=ROWS).contains(&row) || col > COLS {
        return Err("Endereço fora da grade".into());
    }
    Ok((row, col))
}
pub fn parse_range(s: &str) -> Result<Range> {
    let (a, b) = s.split_once(':').unwrap_or((s, s));
    Ok(Range::between(parse_addr(a)?, parse_addr(b)?))
}
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub extras: Extras,
    pub id: u64,
}
impl Snapshot {
    fn size(&self) -> usize {
        self.bytes.len() + serde_json::to_vec(&self.extras).map_or(0, |b| b.len())
    }
}
pub struct Book {
    pub m: UserModel<'static>,
    pub extra: Extras,
    pub path: Option<PathBuf>,
    pub digest: Option<[u8; 32]>,
    pub copy_only: bool,
    pub readonly: bool,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
    pub version: u64,
    next: u64,
    pub saved: u64,
}
impl Book {
    pub fn new(locale: &str) -> Result<Self> {
        let mut m = UserModel::new_empty("Pasta", engine_locale(locale)?, "UTC", "en")?;
        m.rename_sheet(0, "Planilha1")?;
        Ok(Self::from_model(m))
    }
    pub fn from_model(m: UserModel<'static>) -> Self {
        Self {
            m,
            extra: Extras::default(),
            path: None,
            digest: None,
            copy_only: false,
            readonly: false,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            version: 0,
            next: 1,
            saved: 0,
        }
    }
    pub fn finish(&mut self) {
        let sheets = self.m.get_worksheets_properties();
        let rows = self
            .extra
            .filters
            .iter()
            .filter_map(|f| Some((sheets.iter().position(|w| w.sheet_id == f.sheet)? as u32, f)))
            .flat_map(|(s, f)| f.excluded.iter().map(move |r| (s, *r)));
        self.m.finish_transaction(rows)
    }
    pub fn dirty(&self) -> bool {
        self.version != self.saved
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            bytes: self.m.to_bytes(),
            extras: self.extra.clone(),
            id: self.version,
        }
    }
    fn restore(&mut self, s: Snapshot) -> Result {
        self.m = UserModel::from_bytes(&s.bytes, "en")?;
        self.extra = s.extras;
        self.finish();
        self.version = s.id;
        Ok(())
    }
    pub fn change<F>(&mut self, f: F) -> Result
    where
        F: FnOnce(&mut Book) -> Result,
    {
        if self.readonly {
            return Err("Somente leitura".into());
        }
        let old = self.snapshot();
        self.m.pause_evaluation();
        if let Err(e) = f(self).and_then(|_| self.reapply_filters()) {
            self.restore(old)?;
            return Err(e);
        }
        self.finish();
        self.undo.push_back(old);
        self.redo.clear();
        self.trim_history();
        self.version = self.next;
        self.next += 1;
        Ok(())
    }
    pub fn history(&mut self, redo: bool) -> Result {
        if self.readonly {
            return Err("Somente leitura".into());
        }
        let s = if redo {
            self.redo.pop_back()
        } else {
            self.undo.pop_back()
        }
        .ok_or("Histórico vazio")?;
        let current = self.snapshot();
        self.restore(s)?;
        if redo {
            self.undo.push_back(current)
        } else {
            self.redo.push_back(current)
        }
        self.trim_history();
        Ok(())
    }
    fn trim_history(&mut self) {
        let mut bytes = self
            .undo
            .iter()
            .chain(self.redo.iter())
            .map(Snapshot::size)
            .sum::<usize>();
        while bytes > HISTORY {
            let Some(snapshot) = self.undo.pop_front().or_else(|| self.redo.pop_front()) else {
                break;
            };
            bytes -= snapshot.size();
        }
    }
    pub fn ws(&self, s: u32) -> &Worksheet {
        &self.m.get_model().workbook.worksheets[s as usize]
    }
    pub fn val(&self, s: u32, r: i32, c: i32) -> String {
        self.m
            .get_formatted_cell_value(s, r, c)
            .unwrap_or_else(|_| "#REF!".into())
    }
    pub fn raw(&self, s: u32, r: i32, c: i32) -> String {
        self.extra
            .original(self.ws(s).sheet_id, r, c)
            .map(str::to_owned)
            .unwrap_or_else(|| self.m.get_cell_content(s, r, c).unwrap_or_default())
    }
    pub fn set(&mut self, s: u32, r: i32, c: i32, v: &str) -> Result {
        let id = self.ws(s).sheet_id;
        self.extra.validate(id, r, c, v)?;
        let mut text = v.to_owned();
        if self.m.get_cell_style(s, r, c)?.num_fmt == "@" && !v.starts_with('\'') && !v.is_empty() {
            text.insert(0, '\'')
        } else if v.len() > 1 && v.starts_with('0') && v.bytes().all(|c| c.is_ascii_digit()) {
            text.insert(0, '\'')
        }
        if !v.starts_with('=')
            && v.split('/').count() == 3
            && v.chars().all(|c| c.is_ascii_digit() || c == '/')
        {
            return Err("Use data ISO AAAA-MM-DD ou apóstrofo para texto".into());
        }
        text = self.extra.translate(id, r, c, &text)?;
        self.m.set_user_input(s, r, c, &text)?;
        self.extra.record(id, r, c, v, &text);
        Ok(())
    }
    pub fn number(&self, s: u32, r: i32, c: i32) -> Option<f64> {
        match self.m.get_model().get_cell_value_by_index(s, r, c).ok()? {
            CellValue::Number(n) => Some(n),
            _ => None,
        }
    }
    pub fn select(&mut self, s: u32, a: Range) -> Result {
        self.m.set_selected_sheet(s)?;
        self.m.set_selected_cell(a.r, a.c)?;
        self.m.set_selected_range(a.r, a.c, a.end, a.right)
    }
    pub fn clipboard(&mut self, s: u32, a: Range, cut: bool) -> Result<Clip> {
        a.bounded()?;
        self.select(s, a)?;
        let (data, tsv) = self.m.copy_to_clipboard()?.into_parts();
        let mut values = Vec::new();
        let mut formulas = Vec::new();
        let mut styles = Vec::new();
        let mut original = Vec::new();
        for r in a.r..=a.end {
            for c in a.c..=a.right {
                let v = self.m.get_model().get_cell_value_by_index(s, r, c)?;
                values.push(match v {
                    CellValue::String(x)
                        if matches!(
                            self.m.get_cell_type(s, r, c)?,
                            ironcalc::base::types::CellType::ErrorValue
                        ) =>
                    {
                        x
                    }
                    CellValue::String(x) => format!("'{x}"),
                    CellValue::Number(n) => n.to_string().replace(
                        '.',
                        if self.m.get_locale() == "pt" {
                            ","
                        } else {
                            "."
                        },
                    ),
                    CellValue::Boolean(b) => if b { "TRUE" } else { "FALSE" }.into(),
                    CellValue::None => String::new(),
                });
                formulas.push(self.raw(s, r, c));
                styles.push(self.m.get_cell_style(s, r, c)?);
                original.push(
                    self.extra
                        .original(self.ws(s).sheet_id, r, c)
                        .map(str::to_owned),
                );
            }
        }
        let mut notes = Vec::new();
        for note in &self.ws(s).comments {
            let (row, col) = parse_addr(&note.cell_ref)?;
            if a.contains(row, col) {
                notes.push((row - a.r, col - a.c, note.clone()));
            }
        }
        Ok(Clip {
            notes,
            tsv,
            sheet: s,
            id: self.ws(s).sheet_id,
            a,
            cut,
            version: self.version,
            data,
            values,
            formulas,
            styles,
            original,
        })
    }
    pub fn paste(&mut self, p: &Clip, s: u32, r: i32, c: i32, mode: &str) -> Result {
        if p.cut && p.version != self.version {
            return Err("Origem alterada. Recorte novamente".into());
        }
        if self
            .m
            .get_worksheets_properties()
            .get(p.sheet as usize)
            .is_none_or(|w| w.sheet_id != p.id)
        {
            return Err("Aba de origem mudou. Copie novamente".into());
        }
        let a = p.a;
        let w = a.right - a.c + 1;
        let h = a.end - a.r + 1;
        let (hh, ww) = if mode == "transpor" { (w, h) } else { (h, w) };
        let dest = Range {
            r,
            c,
            end: r + hh - 1,
            right: c + ww - 1,
        };
        dest.bounded()?;
        if p.cut && mode != "normal" {
            return Err("Recortar exige colagem normal".into());
        }
        self.select(s, Range::cell(r, c))?;
        if mode == "normal" {
            if p.cut {
                let src_id = self.ws(p.sheet).sheet_id;
                let dest_id = self.ws(s).sheet_id;
                let intersects_metadata = |id, area| {
                    self.extra
                        .tables
                        .iter()
                        .any(|t| t.sheet == id && crate::data::overlap(t.area, area))
                        || self
                            .extra
                            .rules
                            .iter()
                            .any(|v| v.sheet == id && crate::data::overlap(v.area, area))
                };
                if intersects_metadata(src_id, a) || intersects_metadata(dest_id, dest) {
                    return Err("Recorte atravessa tabela ou validação. Converta a tabela ou remova a regra antes de mover suas células".into());
                }
                if !self.extra.expressions.is_empty() {
                    return self.cut_with_table_references(p.sheet, a, s, dest);
                }
            }
            let dest_id = self.ws(s).sheet_id;
            self.extra
                .expressions
                .retain(|e| e.sheet != dest_id || !dest.contains(e.r, e.c));
            self.m
                .paste_from_clipboard(p.sheet, (a.r, a.c, a.end, a.right), &p.data, p.cut)?;
            let mut translator = None;
            for i in 0..h {
                for j in 0..w {
                    let k = (i * w + j) as usize;
                    let rr = r + i;
                    let cc = c + j;
                    if let Some(f) = &p.original[k] {
                        if translator.is_none() {
                            translator =
                                Some(ironcalc::base::Model::from_bytes(&self.m.to_bytes(), "en")?)
                        }
                        use ironcalc::base::expressions::types::CellReferenceIndex as Ref;
                        let v = crate::structure::copied(
                            translator.as_mut().unwrap(),
                            f,
                            Ref {
                                sheet: p.sheet,
                                row: a.r + i,
                                column: a.c + j,
                            },
                            Ref {
                                sheet: s,
                                row: rr,
                                column: cc,
                            },
                        )?;
                        self.set(s, rr, cc, &v)?;
                    } else {
                        self.extra.validate(dest_id, rr, cc, &self.raw(s, rr, cc))?;
                    }
                }
            }
            // The engine clamps copied blocks to used dimensions. Explicitly clear trailing cells.
            for i in 0..h {
                for j in 0..w {
                    if !p
                        .data
                        .get(&(a.r + i))
                        .is_some_and(|row| row.contains_key(&(a.c + j)))
                    {
                        self.m.range_clear_all(&Range::cell(r + i, c + j).area(s))?
                    }
                }
            }
        } else {
            let mut translator = ironcalc::base::Model::from_bytes(&self.m.to_bytes(), "en")?;
            for i in 0..h {
                for j in 0..w {
                    let k = (i * w + j) as usize;
                    let (rr, cc) = if mode == "transpor" {
                        (r + j, c + i)
                    } else {
                        (r + i, c + j)
                    };
                    match mode {
                        "valores" => self.set(s, rr, cc, &p.values[k])?,
                        "formato" => {
                            self.select(s, Range::cell(rr, cc))?;
                            self.m.on_paste_styles(&[vec![p.styles[k].clone()]])?
                        }
                        "formulas" | "transpor" => {
                            let v = &p.formulas[k];
                            if v.starts_with('=') || mode == "transpor" {
                                use ironcalc::base::expressions::types::CellReferenceIndex as Ref;
                                let v = crate::structure::copied(
                                    &mut translator,
                                    v,
                                    Ref {
                                        sheet: p.sheet,
                                        row: a.r + i,
                                        column: a.c + j,
                                    },
                                    Ref {
                                        sheet: s,
                                        row: rr,
                                        column: cc,
                                    },
                                )?;
                                self.set(s, rr, cc, &v)?;
                                if mode == "transpor" {
                                    self.select(s, Range::cell(rr, cc))?;
                                    self.m.on_paste_styles(&[vec![p.styles[k].clone()]])?
                                }
                            }
                        }
                        _ => return Err("Modo de colagem inválido".into()),
                    }
                }
            }
        }
        if matches!(mode, "normal" | "transpor") {
            if p.cut {
                self.clear_notes(p.sheet, a)?;
            }
            self.clear_notes(s, dest)?;
            let notes = self.m.notes_mut(s)?;
            for (i, j, original) in &p.notes {
                let mut note = original.clone();
                let (row, col) = if mode == "transpor" {
                    (r + j, c + i)
                } else {
                    (r + i, c + j)
                };
                note.cell_ref = addr(row, col);
                notes.push(note);
            }
        }
        Ok(())
    }

    pub fn clear_notes(&mut self, s: u32, area: Range) -> Result {
        self.m
            .notes_mut(s)?
            .retain(|n| !parse_addr(&n.cell_ref).is_ok_and(|(r, c)| area.contains(r, c)));
        Ok(())
    }

    pub fn tsv(&mut self, s: u32, r: i32, c: i32, text: &str) -> Result {
        crate::interchange::paste(self, s, r, c, text, b'\t')
    }
}
pub fn engine_locale(locale: &str) -> Result<&'static str> {
    match locale {
        "pt" | "pt-BR" => Ok("pt"),
        "en" | "en-US" => Ok("en"),
        _ => Err("Locales disponíveis: pt-BR, en-US".into()),
    }
}
pub struct Clip {
    pub tsv: String,
    notes: Vec<(i32, i32, ironcalc::base::types::Comment)>,
    sheet: u32,
    id: u32,
    a: Range,
    pub cut: bool,
    version: u64,
    data: ClipboardData,
    values: Vec<String>,
    formulas: Vec<String>,
    styles: Vec<ironcalc::base::types::Style>,
    original: Vec<Option<String>>,
}
