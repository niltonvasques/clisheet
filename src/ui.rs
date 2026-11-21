mod commands;
mod editor;
mod formula;
mod render;
use crate::{book::*, storage};
use crossterm::event::{
    Event, KeyCode as K, KeyEvent, KeyEventKind, KeyModifiers as M, MouseButton as B, MouseEvent,
    MouseEventKind as MK,
};
use editor::Editor;
use ironcalc::base::worksheet::NavigationDirection as Dir;
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use unicode_width::UnicodeWidthStr;
pub const MENUS: [&str; 8] = [
    "Arquivo",
    "Editar",
    "Exibir",
    "Inserir",
    "Formatar",
    "Dados",
    "Fórmulas",
    "Ajuda",
];
// Left column is an executable command. Ellipses identify parameters entered in the command line.
pub const ACTIONS:[&str;8]=[
"abrir|salvar|copia|importar tab utf-8 …|exportar seleção virgula utf-8 …|exportar aba tab utf-8 …|sair",
"editar|desfazer|refazer|copiar|copiar-externo|recortar|colar|colar valores|colar formulas|colar formato|colar transpor|limpar|limpar-formato|limpar-tudo|preencher …|baixo|direita|serie 1|buscar valor …|buscar original …|buscar formula …|substituir seleção antigo => novo|substituir aba antigo => novo|substituir pasta antigo => novo",
"ir …|congelar|descongelar|largura 14|altura 2|autoajustar|ocultar-linhas|mostrar-linhas|ocultar-colunas|mostrar-colunas|proxima-aba|aba-anterior|tema",
"nova-aba|renomear-aba …|duplicar-aba|excluir-aba|mover-aba 1|ocultar-aba|mostrar-aba …|inserir-linhas|inserir-colunas|excluir-linhas|excluir-colunas|link …|nota …|ler-nota|remover-nota",
"negrito|italico|sublinhado|borda|quebra|esquerda|centro|direita-alinhar|formato geral|formato numero|formato moeda|formato percentual|formato data|formato hora|formato texto|formato personalizado …|cor #FFFFFF|fundo #315877|mesclar|desmesclar|condicional duplicados|condicional maior 0|condicional entre 0 10",
"ordenar cab A+|ordenar sem A+|filtrar A …|remover-filtro|tabela …|converter-intervalo|redimensionar-tabela …|coluna-calculada …|registro|totais|renomear-tabela …|renomear-coluna …|validar lista Sim,Não|validar numero 0 100|validar data 2026-01-01 2026-12-31|remover-validacao",
"recalcular|nome pasta …|nome aba …|nomes|excluir-nome pasta …|excluir-nome aba …",
"ajuda"];
#[derive(Clone, Copy)]
pub struct View {
    pub r: i32,
    pub c: i32,
    pub anchor: (i32, i32),
    pub top: i32,
    pub left: i32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            r: 1,
            c: 1,
            anchor: (1, 1),
            top: 1,
            left: 1,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Track {
    pub index: i32,
    pub start: u16,
    pub size: u16,
}
impl Track {
    fn hits(self, n: u16) -> bool {
        n >= self.start && n < self.start + self.size
    }
}
pub enum Mode {
    Nav,
    Edit,
    Prompt,
    Menu,
    Files(bool),
    Confirm(String),
    Help,
}
pub struct App {
    pub book: Book,
    pub sheet: u32,
    pub v: View,
    views: HashMap<u32, View>,
    pub mode: Mode,
    pub editor: Editor,
    pub status: String,
    pub theme: bool,
    pub locale: String,
    pub rows: Vec<Track>,
    pub cols: Vec<Track>,
    tabs: Vec<(Track, u32)>,
    menu_hits: Vec<Track>,
    pub width: u16,
    pub height: u16,
    menu: Option<usize>,
    index: usize,
    offset: usize,
    entries: Vec<String>,
    directory: PathBuf,
    clip: Option<Clip>,
    drag: Option<(i32, u16, i32)>,
    selection_scroll: Option<(i32, i32)>,
    last: Option<(u16, u16, Instant)>,
    pub quit: bool,
    pub osc52: bool,
    pub backup: bool,
    pub bindings: HashMap<String, String>,
    follow: bool,
    edit_origin: Option<(u32, View)>,
    ref_pos: Option<(i32, i32)>,
    ref_span: Option<(usize, usize)>,
    ref_cursor: Option<(i32, i32)>,
    pub journal: PathBuf,
    pub journal_version: Option<u64>,
    after_save: Option<String>,
}
impl App {
    pub fn new(book: Book, locale: &str) -> Self {
        let sheet = book
            .m
            .get_worksheets_properties()
            .iter()
            .position(|w| w.state == "visible")
            .unwrap_or(0) as u32;
        let status = if book.copy_only {
            "Origem externa • use Salvar cópia; objetos podem ser omitidos"
        } else {
            "F1 ajuda   F10 ações"
        };
        Self {
            book,
            sheet,
            v: View::default(),
            views: HashMap::new(),
            mode: Mode::Nav,
            editor: Editor::default(),
            status: status.into(),
            theme: true,
            locale: locale.into(),
            rows: vec![],
            cols: vec![],
            tabs: vec![],
            menu_hits: vec![],
            width: 80,
            height: 24,
            menu: None,
            index: 0,
            offset: 0,
            entries: vec![],
            directory: std::env::current_dir().unwrap_or_else(|_| ".".into()),
            clip: None,
            drag: None,
            selection_scroll: None,
            last: None,
            quit: false,
            osc52: false,
            backup: true,
            bindings: HashMap::new(),
            follow: true,
            edit_origin: None,
            ref_pos: None,
            ref_span: None,
            ref_cursor: None,
            journal: storage::recovery_dir().join(format!(
                "sessao-{}-{}.xlsx",
                std::process::id(),
                chrono::Utc::now().timestamp_micros()
            )),
            journal_version: None,
            after_save: None,
        }
    }
    pub fn selection(&self) -> Range {
        Range::between(self.v.anchor, (self.v.r, self.v.c))
    }
    pub fn focus(&mut self, r: i32, c: i32, extend: bool) {
        self.v.r = r.clamp(1, ROWS);
        self.v.c = c.clamp(1, COLS);
        if !extend {
            self.v.anchor = (self.v.r, self.v.c)
        }
        self.follow = true
    }
    fn jump(&mut self, a: Range) {
        self.focus(a.r, a.c, false);
        self.focus(a.end, a.right, true)
    }
    pub fn switch(&mut self, s: u32) -> Result {
        if self
            .book
            .m
            .get_worksheets_properties()
            .get(s as usize)
            .is_none_or(|w| w.state != "visible")
        {
            return Err("Aba indisponível".into());
        }
        self.views.insert(self.book.ws(self.sheet).sheet_id, self.v);
        self.sheet = s;
        self.v = *self
            .views
            .get(&self.book.ws(s).sheet_id)
            .unwrap_or(&View::default());
        self.follow = true;
        Ok(())
    }
    fn repair_sheet(&mut self, id: u32) {
        let w = self.book.m.get_worksheets_properties();
        self.sheet = w
            .iter()
            .position(|p| p.sheet_id == id && p.state == "visible")
            .or_else(|| w.iter().position(|p| p.state == "visible"))
            .unwrap_or(0) as u32;
        self.follow = true
    }
    fn adjacent(&mut self, d: i32) -> Result {
        let w = self.book.m.get_worksheets_properties();
        let v: Vec<_> = w
            .iter()
            .enumerate()
            .filter(|(_, p)| p.state == "visible")
            .map(|(i, _)| i as u32)
            .collect();
        let n = v.iter().position(|p| *p == self.sheet).unwrap_or(0) as i32;
        self.switch(v[(n + d).rem_euclid(v.len() as i32) as usize])
    }
    fn begin(&mut self, value: String) -> Result {
        if self.book.readonly {
            return Err("Somente leitura".into());
        }
        self.edit_origin = Some((self.sheet, self.v));
        self.editor = Editor::new(&value);
        self.mode = Mode::Edit;
        self.ref_pos = None;
        self.ref_span = None;
        self.ref_cursor = None;
        Ok(())
    }
    fn edit_target(&self) -> (u32, View) {
        self.edit_origin.unwrap_or((self.sheet, self.v))
    }

    fn restore_edit_view(&mut self) -> Result {
        if let Some((s, v)) = self.edit_origin.take() {
            if self.sheet != s {
                self.switch(s)?;
            }
            self.v = v;
            self.follow = true;
        }
        Ok(())
    }

    fn reference_sheet(&mut self, sheet: u32) -> Result {
        self.switch(sheet)?;
        self.ref_pos = None;
        self.ref_cursor = Some((self.v.r, self.v.c));
        Ok(())
    }
    fn prompt(&mut self, s: &str) {
        self.editor = Editor::new(s);
        self.mode = Mode::Prompt
    }
    fn list(&mut self, menu: Option<usize>) {
        self.mode = Mode::Menu;
        self.menu = menu;
        self.offset = 0;
        self.index = 0;
        self.entries = menu
            .map_or_else(|| ACTIONS.join("|"), |m| ACTIONS[m].into())
            .split('|')
            .map(str::to_owned)
            .collect()
    }
    fn files(&mut self, save: bool) -> Result {
        self.mode = Mode::Files(save);
        self.entries = vec!["../".into()];
        let mut paths = std::fs::read_dir(&self.directory)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let n = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    Some(format!("{n}/"))
                } else if p
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("xlsx"))
                {
                    Some(n)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        paths.sort();
        self.entries.extend(paths);
        self.offset = 0;
        self.index = 0;
        self.editor = Editor::default();
        Ok(())
    }
    fn pick(&mut self) -> Result {
        if let Mode::Files(save) = self.mode {
            let input = if self.editor.text.is_empty() {
                self.entries.get(self.index).ok_or("Sem seleção")?.clone()
            } else {
                self.editor.text.clone()
            };
            let p = self.directory.join(input);
            if p.is_dir() {
                self.directory = p.canonicalize().map_err(|e| e.to_string())?;
                return self.files(save);
            }
            if save {
                storage::save(&mut self.book, &p, self.backup)?;
                self.status = format!("Salvo em {}", p.display());
                match self.after_save.take().as_deref() {
                    Some("sair") => self.quit = true,
                    Some("abrir") => return self.files(false),
                    _ => (),
                }
            } else {
                let mut b = storage::open(&p, &self.locale)?;
                b.readonly = self.book.readonly;
                let mut app = Self::new(b, &self.locale);
                app.osc52 = self.osc52;
                app.backup = self.backup;
                app.theme = self.theme;
                app.bindings = self.bindings.clone();
                *self = app
            }
            self.mode = Mode::Nav;
            return Ok(());
        }
        let v = self.entries.get(self.index).ok_or("Ação inválida")?.clone();
        if v.contains('…') {
            self.prompt(&v.replace('…', ""));
            return Ok(());
        }
        if [
            "ordenar ",
            "validar ",
            "condicional maior",
            "condicional entre",
            "largura ",
            "altura ",
            "serie ",
            "substituir ",
            "cor ",
            "fundo ",
            "mover-aba ",
        ]
        .iter()
        .any(|p| v.starts_with(p))
        {
            self.prompt(&v);
            return Ok(());
        }
        self.mode = Mode::Nav;
        self.command(&v)
    }
    fn edit_ref(&mut self, r: i32, c: i32, extend: bool) {
        if let Some((a, b)) = self.ref_span {
            self.editor.anchor = Some(a);
            self.editor.cursor = b
        }
        let origin = if extend {
            self.ref_pos.unwrap_or((r, c))
        } else {
            (r, c)
        };
        let from = self
            .editor
            .anchor
            .unwrap_or(self.editor.cursor)
            .min(self.editor.cursor);
        let mut text = Range::between(origin, (r, c)).address();
        if self.edit_target().0 != self.sheet {
            text = format!(
                "'{}'!{text}",
                self.book.ws(self.sheet).name.replace("'", "''")
            );
        }
        self.editor.replace(&text);
        self.ref_span = Some((from, self.editor.cursor));
        self.ref_pos = Some(origin)
    }
    fn step(&mut self, dr: i32, dc: i32, extend: bool, ctrl: bool) -> Result {
        let mut r = self.v.r;
        let mut c = self.v.c;
        if ctrl {
            let dir = if dr < 0 {
                Dir::Up
            } else if dr > 0 {
                Dir::Down
            } else if dc < 0 {
                Dir::Left
            } else {
                Dir::Right
            };
            (r, c) = self
                .book
                .ws(self.sheet)
                .navigate_to_edge_in_direction(r, c, dir)?
        } else {
            r = (r + dr).clamp(1, ROWS);
            c = (c + dc).clamp(1, COLS);
            while dr != 0 && self.book.ws(self.sheet).is_row_hidden(r)? {
                let next = (r + dr.signum()).clamp(1, ROWS);
                if r == next {
                    break;
                }
                r = next
            }
            while dc != 0 && self.book.ws(self.sheet).is_column_hidden(c)? {
                let next = (c + dc.signum()).clamp(1, COLS);
                if c == next {
                    break;
                }
                c = next
            }
        }
        self.focus(r, c, extend);
        Ok(())
    }
    pub fn event(&mut self, e: Event) {
        let res = match e {
            Event::Key(k) if k.kind != KeyEventKind::Release => self.key(k),
            Event::Mouse(m) => self.mouse(m),
            Event::Paste(text) => match self.mode {
                Mode::Edit | Mode::Prompt | Mode::Files(_) => {
                    self.ref_span = None;
                    self.editor.replace(&text);
                    Ok(())
                }
                Mode::Nav => self
                    .book
                    .change(|b| b.tsv(self.sheet, self.v.r, self.v.c, &text)),
                _ => Ok(()),
            },
            Event::Resize(_, _) => {
                self.follow = true;
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = res {
            self.status = e
        }
    }
    fn key(&mut self, k: KeyEvent) -> Result {
        self.selection_scroll = None;
        let ctrl = k.modifiers.contains(M::CONTROL);
        let shift = k.modifiers.contains(M::SHIFT);
        if k.code == K::Esc {
            self.restore_edit_view()?;
            self.mode = Mode::Nav;
            self.after_save = None;
            self.ref_span = None;
            self.drag = None;
            return Ok(());
        }
        match &self.mode {
            Mode::Confirm(cmd) => {
                let cmd = cmd.clone();
                match k.code {
                    K::Char('d') => {
                        self.mode = Mode::Nav;
                        if cmd == "abrir" {
                            self.files(false)?
                        } else {
                            self.quit = true
                        }
                    }
                    K::Char('s') => {
                        self.after_save = Some(cmd.clone());
                        self.command("salvar")?;
                        if !matches!(self.mode, Mode::Files(_)) {
                            self.quit = cmd == "sair";
                            self.after_save = None;
                            if cmd == "abrir" {
                                self.files(false)?
                            }
                        }
                    }
                    _ => (),
                }
                return Ok(());
            }
            Mode::Help => {
                self.mode = Mode::Nav;
                return Ok(());
            }
            Mode::Edit | Mode::Prompt => {
                if matches!(self.mode, Mode::Edit) && self.editor.text.starts_with('=') {
                    let direction = match k.code {
                        K::F(6) => 1,
                        K::F(7) => -1,
                        K::PageDown if ctrl => 1,
                        K::PageUp if ctrl => -1,
                        _ => 0,
                    };
                    if direction != 0 {
                        self.adjacent(direction)?;
                        self.ref_pos = None;
                        self.ref_cursor = Some((self.v.r, self.v.c));
                        return Ok(());
                    }
                }
                if matches!(self.mode, Mode::Edit)
                    && self.editor.text.starts_with('=')
                    && k.code == K::F(4)
                {
                    self.ref_cursor = if self.ref_cursor.is_some() {
                        None
                    } else {
                        Some((self.v.r, self.v.c))
                    };
                    return Ok(());
                }
                if let Some((r, c)) = self.ref_cursor {
                    let delta = match k.code {
                        K::Up => Some((-1, 0)),
                        K::Down => Some((1, 0)),
                        K::Left => Some((0, -1)),
                        K::Right => Some((0, 1)),
                        _ => None,
                    };
                    if let Some((dr, dc)) = delta {
                        let r = (r + dr).clamp(1, ROWS);
                        let c = (c + dc).clamp(1, COLS);
                        self.edit_ref(r, c, shift);
                        self.focus(r, c, false);
                        self.ref_cursor = Some((r, c));
                        return Ok(());
                    }
                    self.ref_cursor = None
                }
                if k.code == K::Enter && !k.modifiers.contains(M::ALT) {
                    let t = self.editor.text.clone();
                    if matches!(self.mode, Mode::Edit) {
                        let (s, v) = self.edit_target();
                        self.book.change(|b| b.set(s, v.r, v.c, &t))?;
                        self.restore_edit_view()?;
                        self.step(1, 0, false, false)?;
                        self.mode = Mode::Nav
                    } else {
                        self.mode = Mode::Nav;
                        self.command(t.trim())?
                    }
                } else if k.code == K::Tab
                    && matches!(self.mode, Mode::Edit)
                    && self.editor.text.starts_with('=')
                {
                    self.complete()
                } else {
                    self.ref_span = None;
                    self.ref_pos = None;
                    self.editor.key(k)
                }
                return Ok(());
            }
            Mode::Files(_) | Mode::Menu => {
                match k.code {
                    K::Up => self.index = self.index.saturating_sub(1),
                    K::Down => {
                        self.index = (self.index + 1).min(self.entries.len().saturating_sub(1))
                    }
                    K::PageDown => {
                        self.index = (self.index + 10).min(self.entries.len().saturating_sub(1))
                    }
                    K::PageUp => self.index = self.index.saturating_sub(10),
                    K::Enter => return self.pick(),
                    K::Left | K::Right if matches!(self.mode, Mode::Menu) => {
                        let n = (self.menu.unwrap_or(0) as i32
                            + if k.code == K::Right { 1 } else { -1 })
                        .rem_euclid(8);
                        self.list(Some(n as usize))
                    }
                    K::Char(':') if matches!(self.mode, Mode::Menu) => self.prompt(""),
                    _ if matches!(self.mode, Mode::Files(_)) => self.editor.key(k),
                    _ => (),
                }
                return Ok(());
            }
            Mode::Nav => (),
        }
        let custom = format!(
            "{}{}{}",
            if ctrl { "Ctrl+" } else { "" },
            if shift { "Shift+" } else { "" },
            match k.code {
                K::Char(c) => c.to_string(),
                K::F(n) => format!("F{n}"),
                _ => format!("{:?}", k.code),
            }
        );
        if let Some(cmd) = self.bindings.get(&custom).cloned() {
            return self.command(&cmd);
        }
        if ctrl {
            let action = match k.code {
                K::Char('s') => "salvar",
                K::Char('o') => "abrir",
                K::Char('z') => "desfazer",
                K::Char('y') => "refazer",
                K::Char('c') => "copiar",
                K::Char('x') => "recortar",
                K::Char('v') => "colar",
                K::Char('q') => "sair",
                K::PageDown => "proxima-aba",
                K::PageUp => "aba-anterior",
                K::Char('f') => {
                    self.prompt("buscar valor ");
                    return Ok(());
                }
                K::Char('g') => {
                    self.prompt("ir ");
                    return Ok(());
                }
                _ => "",
            };
            if !action.is_empty() {
                return self.command(action);
            }
        }
        match k.code {
            K::F(1) => self.mode = Mode::Help,
            K::F(10) => self.list(None),
            K::F(2) => return self.command("editar"),
            K::F(5) => return self.command("recalcular"),
            K::Char(':') => self.prompt(""),
            K::Char(c) if !ctrl && !c.is_control() => return self.begin(c.to_string()),
            K::Enter | K::Down => self.step(1, 0, shift, ctrl)?,
            K::Up => self.step(-1, 0, shift, ctrl)?,
            K::Tab | K::Right => self.step(0, 1, shift, ctrl)?,
            K::BackTab => self.step(0, -1, false, false)?,
            K::Left => self.step(0, -1, shift, ctrl)?,
            K::PageUp | K::PageDown => self.step(
                (self.rows.len().max(1) as i32) * if k.code == K::PageUp { -1 } else { 1 },
                0,
                shift,
                false,
            )?,
            K::Home => self.focus(if ctrl { 1 } else { self.v.r }, 1, shift),
            K::End => {
                let d = self.book.ws(self.sheet).dimension();
                self.focus(if ctrl { d.max_row } else { self.v.r }, d.max_column, shift)
            }
            K::Delete => return self.command("limpar"),
            _ => (),
        }
        Ok(())
    }
    fn complete(&mut self) {
        let prefix = &self.editor.text[..self.editor.cursor];
        let i = prefix
            .rfind(|c: char| !c.is_alphabetic())
            .map_or(0, |j| j + prefix[j..].chars().next().unwrap().len_utf8());
        let token = prefix[i..].to_uppercase();
        if token.is_empty() {
            return;
        }
        let names = commands::FUNCTIONS
            .split_whitespace()
            .filter(|f| f.starts_with(&token))
            .collect::<Vec<_>>();
        if let Some(name) = names.first() {
            self.editor.anchor = Some(i);
            self.editor.replace(&format!("{name}("));
            self.status = names.join("  ")
        }
    }
    fn scroll(&mut self, dr: i32, dc: i32) {
        self.v.top = (self.v.top + dr).clamp(1, ROWS);
        self.v.left = (self.v.left + dc).clamp(1, COLS);
        self.follow = false
    }
    pub fn advance_drag(&mut self) -> Result<bool> {
        let Some((dr, dc)) = self.selection_scroll else {
            return Ok(false);
        };
        let old = (self.v.r, self.v.c);
        self.step(dr, dc, true, false)?;
        Ok(old != (self.v.r, self.v.c))
    }

    fn mouse(&mut self, m: MouseEvent) -> Result {
        if !matches!(m.kind, MK::Moved) {
            self.selection_scroll = None;
        }
        let (x, y) = (m.column, m.row);
        let down = m.kind == MK::Down(B::Left);
        let shift = m.modifiers.contains(M::SHIFT);
        if matches!(self.mode, Mode::Menu | Mode::Files(_)) {
            let len = self.entries.len();
            match m.kind {
                MK::ScrollDown => self.index = (self.index + 3).min(len.saturating_sub(1)),
                MK::ScrollUp => self.index = self.index.saturating_sub(3),
                MK::Down(B::Left) if y >= 5 && y < self.height.saturating_sub(3) => {
                    let i = self.offset + (y - 5) as usize;
                    if i < len {
                        self.index = i;
                        return self.pick();
                    }
                }
                _ => (),
            }
            return Ok(());
        }
        if matches!(self.mode, Mode::Confirm(_)) && down && y == self.height.saturating_sub(1) {
            let key = if x < 12 {
                's'
            } else if x < 26 {
                'd'
            } else {
                return self.key(KeyEvent::from(K::Esc));
            };
            return self.key(KeyEvent::from(K::Char(key)));
        }
        if matches!(self.mode, Mode::Confirm(_)) {
            return Ok(());
        }
        if matches!(self.mode, Mode::Help) && down {
            self.mode = Mode::Nav;
            return Ok(());
        }
        if matches!(
            m.kind,
            MK::ScrollDown | MK::ScrollUp | MK::ScrollLeft | MK::ScrollRight
        ) {
            let d = if matches!(m.kind, MK::ScrollDown | MK::ScrollRight) {
                3
            } else {
                -3
            };
            if shift || matches!(m.kind, MK::ScrollLeft | MK::ScrollRight) {
                self.scroll(0, d)
            } else {
                self.scroll(d, 0)
            }
            return Ok(());
        }
        if let Some((col, start, w)) = self.drag {
            if m.kind == MK::Up(B::Left) {
                self.drag = None;
                let width = (w + i32::from(x) - i32::from(start)).clamp(2, 180) as f64;
                return self.book.change(|b| {
                    b.m.set_columns_width(
                        self.sheet,
                        col,
                        col,
                        width * ironcalc::base::COLUMN_WIDTH_FACTOR,
                    )
                });
            } else {
                return Ok(());
            }
        }
        let row = self.rows.iter().find(|t| t.hits(y)).map(|t| t.index);
        let col = self.cols.iter().find(|t| t.hits(x)).copied();
        if matches!(self.mode, Mode::Edit) {
            if self.editor.text.starts_with('=') && down && y == self.height.saturating_sub(3) {
                if let Some((_, s)) = self.tabs.iter().find(|(t, s)| t.hits(x) && *s != u32::MAX) {
                    return self.reference_sheet(*s);
                }
            }
            if let (Some(r), Some(c)) = (row, col) {
                if self.editor.text.starts_with('=') && (down || m.kind == MK::Drag(B::Left)) {
                    self.edit_ref(r, c.index, m.kind == MK::Drag(B::Left));
                }
            }
            return Ok(());
        }
        if !matches!(self.mode, Mode::Nav) {
            return Ok(());
        }
        if down && y == 1 {
            if let Some(t) = self.menu_hits.iter().find(|t| t.hits(x)) {
                self.list(Some(t.index as usize))
            }
            return Ok(());
        }
        if down && y == 2 {
            if x < 13 {
                self.prompt("ir ")
            } else {
                return self.command("editar");
            }
            return Ok(());
        }
        if down && y == self.height.saturating_sub(3) {
            if let Some((_, s)) = self.tabs.iter().find(|(t, _)| t.hits(x)) {
                let s = *s;
                if s == u32::MAX {
                    return self.command("nova-aba");
                }
                return self.switch(s);
            }
        }
        let double = down
            && self.last.is_some_and(|(a, b, t)| {
                a == x && b == y && t.elapsed() < Duration::from_millis(400)
            });
        if down {
            self.last = Some((x, y, Instant::now()))
        }
        if down && y == 3 {
            if let Some(t) = col {
                if x + 1 == t.start + t.size {
                    if double {
                        self.autofit(t.index)?
                    } else {
                        self.drag = Some((t.index, x, i32::from(t.size)))
                    }
                } else if let Some(filter) = self
                    .book
                    .extra
                    .filters
                    .iter()
                    .find(|f| f.sheet == self.book.ws(self.sheet).sheet_id && f.col == t.index)
                {
                    let a = filter.area;
                    let cmd = format!(
                        "filtrar {} {}",
                        ironcalc::base::expressions::utils::number_to_column(t.index).unwrap(),
                        filter.condition
                    );
                    self.jump(a);
                    self.prompt(&cmd);
                } else {
                    self.focus(1, t.index, false);
                    self.v.anchor = (ROWS, t.index);
                    self.follow = false
                }
            }
            return Ok(());
        }
        if down && x < 6 {
            if let Some(r) = row {
                self.focus(r, 1, false);
                self.v.anchor = (r, COLS);
                self.follow = false
            }
            return Ok(());
        }
        if m.kind == MK::Drag(B::Left) {
            let dr = if y >= self.height.saturating_sub(4) {
                1
            } else if y <= 4 {
                -1
            } else {
                0
            };
            let dc = if x >= self.width.saturating_sub(1) {
                1
            } else if x <= 6 {
                -1
            } else {
                0
            };
            let track_at = |tracks: &[Track], coord: u16| {
                tracks
                    .iter()
                    .find(|t| t.hits(coord))
                    .or_else(|| {
                        if tracks.first().is_some_and(|t| coord < t.start) {
                            tracks.first()
                        } else {
                            tracks.last()
                        }
                    })
                    .map(|t| t.index)
            };
            if let (Some(r), Some(c)) = (track_at(&self.rows, y), track_at(&self.cols, x)) {
                self.focus(r, c, true);
                if (dr, dc) != (0, 0) {
                    self.selection_scroll = Some((dr, dc));
                    self.advance_drag()?;
                }
            }
            return Ok(());
        }
        if let (Some(r), Some(c)) = (row, col) {
            let (r, c) = self
                .book
                .ws(self.sheet)
                .merged_cells
                .iter()
                .find(|m| {
                    r >= m.row
                        && r < m.row + m.height
                        && c.index >= m.column
                        && c.index < m.column + m.width
                })
                .map_or((r, c.index), |m| (m.row, m.column));
            if down {
                self.focus(r, c, shift);
                if double {
                    return self.command("editar");
                }
            } else if m.kind == MK::Down(B::Right) {
                self.focus(r, c, false);
                self.list(Some(1))
            }
        }
        Ok(())
    }
    fn autofit(&mut self, col: i32) -> Result {
        let mut width = 3;
        for r in self.book.ws(self.sheet).sheet_data.rows() {
            width =
                width.max(UnicodeWidthStr::width(self.book.val(self.sheet, r, col).as_str()) + 2)
        }
        self.book.change(|b| {
            b.m.set_columns_width(
                self.sheet,
                col,
                col,
                width.min(180) as f64 * ironcalc::base::COLUMN_WIDTH_FACTOR,
            )
        })
    }
}
