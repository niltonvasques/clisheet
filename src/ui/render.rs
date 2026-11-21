use super::*;
use ironcalc::base::types::HorizontalAlignment;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
// Dim bright workbook fills while keeping their hue. Neutral white uses the
// same background as an unformatted dark-mode cell.
fn dark_fill(color: Color, base: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return base;
    };
    if (r, g, b) == (255, 255, 255) {
        return base;
    }
    let peak = u16::from(r.max(g).max(b)).max(85);
    let dim = |v: u8| (u16::from(v) * 85 / peak) as u8;
    Color::Rgb(dim(r), dim(g), dim(b))
}

fn luminance(r: u8, g: u8, b: u8) -> f64 {
    let linear = |v: u8| {
        let n = f64::from(v) / 255.;
        if n <= 0.04045 {
            n / 12.92
        } else {
            ((n + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn readable_ink(style: Style) -> Style {
    let (Some(Color::Rgb(mut r, mut g, mut b)), Some(Color::Rgb(br, bg, bb))) =
        (style.fg, style.bg)
    else {
        return style;
    };
    let threshold = 4.5 * (luminance(br, bg, bb) + 0.05) - 0.05;
    let lighten = |v: u8| v + (255 - v).div_ceil(3);
    while luminance(r, g, b) < threshold && (r, g, b) != (255, 255, 255) {
        (r, g, b) = (lighten(r), lighten(g), lighten(b));
    }
    style.fg(Color::Rgb(r, g, b))
}
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() && c != '\n' {
                '�'
            } else {
                c
            }
        })
        .collect()
}
fn put(f: &mut Frame, x: u16, y: u16, w: u16, h: u16, text: &str, style: Style) {
    if w > 0 && h > 0 {
        f.render_widget(
            Paragraph::new(clean(text)).style(style),
            Rect::new(x, y, w, h).intersection(f.area()),
        )
    }
}
fn tracks(
    first: i32,
    frozen: i32,
    last: i32,
    start: u16,
    limit: u16,
    mut size: impl FnMut(i32) -> u16,
) -> Vec<Track> {
    let mut out = Vec::new();
    let mut pos = start;
    let mut i = 1;
    while pos < limit && i <= last {
        if i > frozen && i < first {
            i = first
        }
        let n = size(i);
        if n > 0 {
            let width = n.min(limit - pos);
            out.push(Track {
                index: i,
                start: pos,
                size: width,
            });
            pos += width
        }
        i += 1
    }
    out
}
const HELP:&str="Setas   mover    Shift+setas   selecionar    Ctrl+setas   limite da região\nHome/End   primeira/última coluna usada    Ctrl+Home/End   início/fim da pasta na aba\nF2 ou barra fx   editar    Digitar   substituir    Alt+Enter   nova linha\nEnter/Tab   avançar    Esc   cancelar    Delete   limpar conteúdo\nCtrl+S/O   salvar/abrir    Ctrl+Z/Y   desfazer/refazer    Ctrl+F   buscar\nCtrl+C/X/V   clipboard interno    Cole TSV pelo comando Colar do terminal\nF10   ações    :   digitar comando    Ctrl+G   ir para endereço ou intervalo\nClique e arraste   selecionar    Shift+roda   rolagem horizontal\nArraste o separador do cabeçalho para largura. Duplo clique ajusta ao conteúdo.\nFórmula em edição: clique ou arraste para inserir uma referência. Tab completa função. F6/F7 escolhe a aba.\nApóstrofo força texto. Datas em AAAA-MM-DD. Cole e edite inclusive em linhas ocultas.\nExcel externo: Salvar cópia pode omitir objetos e metadados.\nEsc fecha a ajuda.";
impl App {
    pub fn draw(&mut self, f: &mut Frame) {
        let rect = f.area();
        let (w, h) = (rect.width, rect.height);
        self.width = w;
        self.height = h;
        let (base, ink, muted, bright) = if self.theme {
            (
                Color::White,
                Color::Black,
                Color::Rgb(203, 215, 209),
                Color::Rgb(13, 94, 75),
            )
        } else {
            (
                Color::Rgb(22, 29, 34),
                Color::Rgb(224, 232, 226),
                Color::Rgb(51, 66, 69),
                Color::Rgb(92, 207, 166),
            )
        };
        let normal = Style::default().bg(base).fg(ink);
        let bar = normal.bg(muted);
        let active = normal
            .fg(bright)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        f.render_widget(Block::default().style(normal), rect);
        if w < 20 || h < 9 {
            put(f, 0, 0, w, 1, "Amplie o terminal", active);
            return;
        }
        let name = self
            .book
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or("Pasta nova".into(), |p| p.to_string_lossy());
        put(
            f,
            0,
            0,
            w,
            1,
            &format!(
                " clisheet   {}{}{}{}",
                name,
                if self.book.dirty() { " *" } else { "" },
                if self.book.readonly {
                    "   [SOMENTE LEITURA]"
                } else {
                    ""
                },
                if self.book.copy_only {
                    "   [SALVAR CÓPIA]"
                } else {
                    ""
                }
            ),
            bar.add_modifier(Modifier::BOLD),
        );
        self.menu_hits.clear();
        let mut x = 0;
        for (i, m) in MENUS.iter().enumerate() {
            let n = UnicodeWidthStr::width(*m) as u16 + 2;
            if x + n > w {
                break;
            }
            put(f, x, 1, n, 1, &format!(" {m}"), normal);
            self.menu_hits.push(Track {
                index: i as i32,
                start: x,
                size: n,
            });
            x += n
        }
        let a = self.selection();
        put(f, 0, 2, 13, 1, &format!(" {}", a.address()), active);
        let raw = self.book.raw(self.sheet, self.v.r, self.v.c);
        let editing = matches!(self.mode, Mode::Edit);
        let (edit_sheet, edit_view) = self.edit_target();
        if editing {
            self.editor_line(f, 13, 2, w - 13, "ƒx ", bar)
        } else {
            put(
                f,
                13,
                2,
                w - 13,
                1,
                &format!("ƒx {}", raw.replace('\n', " ↵ ")),
                bar,
            )
        }
        let s = self.sheet;
        let fr = self.book.m.get_frozen_rows_count(s).unwrap_or(0);
        let fc = self.book.m.get_frozen_columns_count(s).unwrap_or(0);
        let row_size = |i| {
            if self.book.ws(s).is_row_hidden(i).unwrap_or(false) {
                0
            } else {
                (self.book.m.get_row_height(s, i).unwrap_or(25.) / 25.)
                    .round()
                    .clamp(1., 40.) as u16
            }
        };
        let col_size = |i| {
            if self.book.ws(s).is_column_hidden(i).unwrap_or(false) {
                0
            } else {
                (self.book.m.get_column_width(s, i).unwrap_or(80.)
                    / ironcalc::base::COLUMN_WIDTH_FACTOR)
                    .round()
                    .clamp(2., 200.) as u16
            }
        };
        if self.follow {
            if self.v.r > fr {
                self.v.top = self.v.top.min(self.v.r)
            }
            if self.v.c > fc {
                self.v.left = self.v.left.min(self.v.c)
            }
        }
        self.rows = tracks(self.v.top, fr, ROWS, 4, h - 3, row_size);
        self.cols = tracks(self.v.left, fc, COLS, 6, w, col_size);
        if self.follow && self.v.r > fr && !self.rows.iter().any(|t| t.index == self.v.r) {
            self.v.top = self.v.r;
            let mut room = h
                .saturating_sub(
                    7 + self
                        .rows
                        .iter()
                        .filter(|t| t.index <= fr)
                        .map(|t| t.size)
                        .sum::<u16>(),
                )
                .saturating_sub(row_size(self.v.r));
            while self.v.top > fr + 1 {
                let n = row_size(self.v.top - 1);
                if n > room {
                    break;
                }
                room -= n;
                self.v.top -= 1
            }
            self.rows = tracks(self.v.top, fr, ROWS, 4, h - 3, row_size)
        }
        if self.follow && self.v.c > fc && !self.cols.iter().any(|t| t.index == self.v.c) {
            self.v.left = self.v.c;
            let mut room = w
                .saturating_sub(
                    6 + self
                        .cols
                        .iter()
                        .filter(|t| t.index <= fc)
                        .map(|t| t.size)
                        .sum::<u16>(),
                )
                .saturating_sub(col_size(self.v.c));
            while self.v.left > fc + 1 {
                let n = col_size(self.v.left - 1);
                if n > room {
                    break;
                }
                room -= n;
                self.v.left -= 1
            }
            self.cols = tracks(self.v.left, fc, COLS, 6, w, col_size)
        }
        self.follow = false;
        let argument_help = self.argument_help();
        let references = self.references();
        let ws = self.book.ws(s);
        let id = ws.sheet_id;
        for t in &self.cols {
            let selected = (a.c..=a.right).contains(&t.index);
            let filter = self
                .book
                .extra
                .filters
                .iter()
                .any(|f| f.sheet == id && f.col == t.index);
            let title = format!(
                "{}{}{}",
                if selected { "›" } else { " " },
                ironcalc::base::expressions::utils::number_to_column(t.index).unwrap(),
                if filter { " ▼" } else { "" }
            );
            put(
                f,
                t.start,
                3,
                t.size,
                1,
                &title,
                if selected { active.bg(muted) } else { bar },
            );
            put(f, t.start + t.size - 1, 3, 1, 1, "│", bar)
        }
        let mut merged = std::collections::HashSet::new();
        for row in &self.rows {
            let r = row.index;
            put(
                f,
                0,
                row.start,
                6,
                row.size,
                &format!(
                    "{}{r:>4}",
                    if (a.r..=a.end).contains(&r) {
                        "›"
                    } else {
                        " "
                    }
                ),
                if (a.r..=a.end).contains(&r) {
                    active.bg(muted)
                } else {
                    bar
                },
            );
            for col in &self.cols {
                let mut rr = r;
                let mut cc = col.index;
                let mut x = col.start;
                let mut y = row.start;
                let mut width = col.size;
                let mut height = row.size;
                if let Some(m) = ws.merged_cells.iter().find(|m| {
                    r >= m.row && r < m.row + m.height && cc >= m.column && cc < m.column + m.width
                }) {
                    rr = m.row;
                    cc = m.column;
                    if !merged.insert((rr, cc)) {
                        continue;
                    }
                    let xs = self
                        .cols
                        .iter()
                        .filter(|t| t.index >= cc && t.index < cc + m.width)
                        .collect::<Vec<_>>();
                    let ys = self
                        .rows
                        .iter()
                        .filter(|t| t.index >= rr && t.index < rr + m.height)
                        .collect::<Vec<_>>();
                    x = xs[0].start;
                    y = ys[0].start;
                    width = xs.iter().map(|t| t.size).sum();
                    height = ys.iter().map(|t| t.size).sum();
                }
                let selected = a.contains(rr, cc);
                let current = (rr, cc) == (self.v.r, self.v.c);
                let mut style = normal;
                let mut align = Alignment::Left;
                let mut wrap = false;
                let mut border = false;
                if let Some(t) = self
                    .book
                    .extra
                    .tables
                    .iter()
                    .find(|t| t.sheet == id && t.area.contains(rr, cc))
                {
                    if rr == t.area.r || t.total && rr == t.area.end {
                        style = bar.add_modifier(Modifier::BOLD)
                    } else if (rr - t.area.r) % 2 == 0 {
                        style = style.bg(if self.theme {
                            Color::Rgb(231, 240, 235)
                        } else {
                            Color::Rgb(30, 43, 44)
                        })
                    }
                }
                if ws.cell(rr, cc).is_some() || !ws.conditional_formatting.is_empty() {
                    if let Ok(st) = self
                        .book
                        .m
                        .get_model()
                        .get_extended_style_for_cell(s, rr, cc)
                    {
                        let st = st.style;
                        if st.font.b {
                            style = style.add_modifier(Modifier::BOLD)
                        }
                        if st.font.i {
                            style = style.add_modifier(Modifier::ITALIC)
                        }
                        if st.font.u {
                            style = style.add_modifier(Modifier::UNDERLINED)
                        }
                        for (color, fg) in [(&st.font.color, true), (&st.fill.color, false)] {
                            if !color.is_none() {
                                if let Ok(rgb) = self.book.m.resolve_color(color).parse::<Color>() {
                                    style = if fg {
                                        style.fg(rgb)
                                    } else if self.theme {
                                        style.bg(rgb)
                                    } else {
                                        style.bg(dark_fill(rgb, base))
                                    }
                                }
                            }
                        }
                        let hor = st.alignment.as_ref().map(|a| &a.horizontal);
                        align = match hor {
                            Some(HorizontalAlignment::Center) => Alignment::Center,
                            Some(HorizontalAlignment::Left) => Alignment::Left,
                            Some(HorizontalAlignment::Right) => Alignment::Right,
                            _ if self.book.number(s, rr, cc).is_some() => Alignment::Right,
                            _ => Alignment::Left,
                        };
                        wrap = st.alignment.is_some_and(|a| a.wrap_text);
                        border = st.border.right.is_some();
                    }
                }
                let referenced = references.iter().any(|a| a.contains(rr, cc));
                if referenced {
                    style = style.fg(bright).add_modifier(Modifier::UNDERLINED)
                }
                if !self.theme && (referenced || style.fg != normal.fg) {
                    style = readable_ink(style);
                }
                if selected {
                    style = style.add_modifier(Modifier::REVERSED)
                }
                if current {
                    style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                }
                let value = if editing && (s, rr, cc) == (edit_sheet, edit_view.r, edit_view.c) {
                    clean(&self.editor.text)
                } else {
                    clean(&self.book.val(s, rr, cc))
                };
                let mut p = Paragraph::new(value).style(style).alignment(align);
                if wrap {
                    p = p.wrap(Wrap { trim: false })
                }
                f.render_widget(p, Rect::new(x + 1, y, width.saturating_sub(2), height));
                put(
                    f,
                    x,
                    y,
                    1,
                    height,
                    if current {
                        "▸"
                    } else if referenced {
                        "ƒ"
                    } else if selected {
                        "·"
                    } else {
                        " "
                    },
                    style,
                );
                put(
                    f,
                    x + width - 1,
                    y,
                    1,
                    height,
                    if border { "┃" } else { "│" },
                    normal.fg(muted),
                );
            }
        }
        let props = self.book.m.get_worksheets_properties();
        let visible = props
            .iter()
            .enumerate()
            .filter(|(_, p)| p.state == "visible")
            .collect::<Vec<_>>();
        self.tabs.clear();
        let mut x = 0;
        let start = if visible
            .iter()
            .take_while(|(i, _)| *i != s as usize)
            .map(|(_, p)| p.name.width() + 4)
            .sum::<usize>()
            > usize::from(w.saturating_sub(20))
        {
            visible
                .iter()
                .position(|(i, _)| *i == s as usize)
                .unwrap_or(0)
        } else {
            0
        };
        for (i, p) in visible.into_iter().skip(start) {
            let label = format!(" {}{} ", if i == s as usize { "● " } else { "" }, p.name);
            let size = (label.width() as u16).min(w.saturating_sub(x + 4));
            if size == 0 {
                break;
            }
            put(
                f,
                x,
                h - 3,
                size,
                1,
                &label,
                if i == s as usize { active } else { bar },
            );
            self.tabs.push((
                Track {
                    index: 0,
                    start: x,
                    size,
                },
                i as u32,
            ));
            x += size
        }
        put(f, x, h - 3, 3, 1, " + ", bar);
        self.tabs.push((
            Track {
                index: 0,
                start: x,
                size: 3,
            },
            u32::MAX,
        ));
        let mut sum = 0.;
        let mut count = 0;
        let mut numbers = 0;
        let mut visit = |r, c| {
            if !self.book.raw(s, r, c).is_empty() {
                count += 1;
                if let Some(n) = self.book.number(s, r, c) {
                    sum += n;
                    numbers += 1
                }
            }
        };
        if i64::from(a.end - a.r + 1) * i64::from(a.right - a.c + 1) < 3000 {
            for r in a.r..=a.end {
                for c in a.c..=a.right {
                    visit(r, c)
                }
            }
        } else {
            for r in ws.sheet_data.rows() {
                if r >= a.r && r <= a.end {
                    for (c, _) in ws.sheet_data.cells_in_row(r) {
                        if a.contains(r, c) {
                            visit(r, c)
                        }
                    }
                }
            }
        }
        let mode = if editing && (self.ref_span.is_some() || self.ref_cursor.is_some()) {
            "REFERÊNCIA"
        } else if editing {
            "EDIÇÃO"
        } else {
            "NAVEGAR"
        };
        use ironcalc::base::{
            expressions::token::Error,
            types::{Cell, FormulaValue},
        };
        let cell = ws.cell(self.v.r, self.v.c);
        let result = match cell.as_deref() {
            Some(Cell::CellFormula { v, .. } | Cell::ArrayFormula { v, .. }) => match v {
                FormulaValue::Unevaluated => "pendente",
                FormulaValue::Error {
                    ei: Error::NIMPL, ..
                } => "função indisponível",
                FormulaValue::Error {
                    ei: Error::NAME, ..
                } => "nome/função desconhecido",
                FormulaValue::Error { .. } => "erro de fórmula",
                _ if editing => "edição pendente",
                _ => "calculado",
            },
            _ if editing => "edição pendente",
            _ => "atualizado",
        };
        let filter = self
            .book
            .extra
            .filters
            .iter()
            .find(|p| p.sheet == id)
            .map(|f| {
                let last = self.book.last_filter_record(s, f.area);
                format!(
                    " Filtro {}/{}",
                    last - f.area.r
                        - ws.rows
                            .iter()
                            .filter(|row| row.hidden && row.r > f.area.r && row.r <= last)
                            .count() as i32,
                    last - f.area.r
                )
            })
            .unwrap_or_default();
        put(
            f,
            0,
            h - 2,
            w,
            1,
            &format!(
                " {mode}  {}  Σ {sum:.2}  Média {}  Cont {}  {result}{filter}",
                a.address(),
                if numbers > 0 {
                    format!("{:.2}", sum / numbers as f64)
                } else {
                    "—".into()
                },
                count
            ),
            bar,
        );
        put(
            f,
            0,
            h - 1,
            w,
            1,
            argument_help
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(&self.status),
            normal.fg(bright),
        );
        match self.mode {
            Mode::Nav | Mode::Edit => (),
            Mode::Prompt => self.editor_line(f, 0, h - 1, w, ": ", bar),
            Mode::Help => {
                let boxarea = Rect::new(1, 2, w - 2, h - 4);
                f.render_widget(Clear, boxarea);
                f.render_widget(
                    Paragraph::new(HELP)
                        .block(
                            Block::default()
                                .title(" Ajuda do clisheet ")
                                .borders(Borders::ALL),
                        )
                        .style(bar)
                        .wrap(Wrap { trim: false }),
                    boxarea,
                )
            }
            Mode::Confirm(_) => put(
                f,
                0,
                h - 1,
                w,
                1,
                "[S] Salvar  [D] Descartar  [Esc] Cancelar",
                active.bg(muted),
            ),
            Mode::Menu | Mode::Files(_) => {
                let files = matches!(self.mode, Mode::Files(_));
                let title = match self.mode {
                    Mode::Files(true) => "Salvar cópia • caminho ou diretório",
                    Mode::Files(false) => "Abrir XLSX • caminho ou diretório",
                    _ => self
                        .menu
                        .map_or("Ações • ← → menus • : comando", |i| MENUS[i]),
                };
                let area = Rect::new(1, 2, w - 2, h - 4);
                f.render_widget(Clear, area);
                f.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(title)
                        .style(bar),
                    area,
                );
                if files {
                    self.editor_line(f, 2, 3, w - 4, "Caminho: ", bar);
                    put(
                        f,
                        2,
                        4,
                        w - 4,
                        1,
                        &self.directory.display().to_string(),
                        normal,
                    )
                } else {
                    put(
                        f,
                        2,
                        3,
                        w - 4,
                        1,
                        "↑ ↓ escolher   Enter executar   Esc fechar",
                        bar,
                    )
                }
                let n = usize::from(h.saturating_sub(8)).max(1);
                self.offset = self.offset.min(self.index);
                if self.index >= self.offset + n {
                    self.offset = self.index + 1 - n
                }
                for (i, item) in self.entries.iter().enumerate().skip(self.offset).take(n) {
                    put(
                        f,
                        2,
                        (5 + i - self.offset) as u16,
                        w - 4,
                        1,
                        &format!("{} {}", if i == self.index { "▸" } else { " " }, item),
                        if i == self.index {
                            active.bg(muted)
                        } else {
                            bar
                        },
                    )
                }
            }
        }
    }
    fn editor_line(&self, f: &mut Frame, x: u16, y: u16, w: u16, prefix: &str, style: Style) {
        let buf = &self.editor;
        let before = clean(&buf.text[..buf.cursor]).replace('\n', "↵");
        let available = usize::from(w).saturating_sub(prefix.width() + 1);
        let mut start = 0;
        let mut width = before.width();
        for (i, ch) in before.char_indices() {
            if width <= available {
                break;
            }
            width = width.saturating_sub(unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0));
            start = i + ch.len_utf8()
        }
        let shown = format!(
            "{}{}{}",
            prefix,
            &before[start..],
            clean(&buf.text[buf.cursor..]).replace('\n', "↵")
        );
        put(f, x, y, w, 1, &shown, style);
        if let Some(a) = buf.anchor {
            let aw = clean(&buf.text[..a]).replace('\n', "↵").width();
            let cw = before.width();
            let clipped = before[..start].width();
            let l = aw.min(cw).saturating_sub(clipped);
            let n = aw.max(cw).saturating_sub(clipped).saturating_sub(l);
            let rect = Rect::new(
                x + prefix.width() as u16 + l.min(usize::from(w)) as u16,
                y,
                n.min(usize::from(w)) as u16,
                1,
            )
            .intersection(Rect::new(x, y, w, 1));
            f.buffer_mut()
                .set_style(rect, style.add_modifier(Modifier::REVERSED))
        }
        f.set_cursor_position((
            x + (prefix.width() + width).min(usize::from(w.saturating_sub(1))) as u16,
            y,
        ))
    }
}
