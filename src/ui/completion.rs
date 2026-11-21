use super::*;
use std::ops::Range as Span;

pub(super) struct Suggestions {
    options: Vec<&'static str>,
    selected: usize,
    span: Span<usize>,
}

impl Suggestions {
    fn for_editor(editor: &Editor) -> Self {
        let before = &editor.text[..editor.cursor];
        let start = before.trim_end_matches(|c: char| !c.is_whitespace()).len();
        let end = editor.cursor
            + editor.text[editor.cursor..]
                .find(char::is_whitespace)
                .unwrap_or(editor.text.len() - editor.cursor);
        let words: Vec<_> = before[..start].split_whitespace().collect();
        let actions = ACTIONS.iter().flat_map(|group| group.split('|'));
        let mut options: Vec<_> = match words.as_slice() {
            [] => actions
                .filter_map(|a| a.split_whitespace().next())
                .collect(),
            ["importar"] | ["exportar", "seleção" | "aba"] => {
                vec!["csv", "virgula", "ponto-e-virgula", "tab", "tsv"]
            }
            ["importar", _] | ["exportar", _, _] => vec!["latin1", "utf-8"],
            [cmd @ ("buscar" | "colar" | "formato" | "condicional" | "validar" | "ordenar"
            | "substituir" | "nome" | "excluir-nome" | "exportar")] => {
                let mut values: Vec<_> = actions
                    .filter_map(|a| {
                        let mut parts = a.split_whitespace();
                        if parts.next() == Some(*cmd) {
                            parts.next()
                        } else {
                            None
                        }
                    })
                    .collect();
                if *cmd == "condicional" {
                    values.push("menor")
                }
                values
            }
            _ => Vec::new(),
        };
        let prefix = before[start..].to_lowercase();
        options.retain(|v| v.starts_with(&prefix));
        options.sort_unstable();
        options.dedup();
        Self {
            options,
            selected: 0,
            span: start..end,
        }
    }

    pub(super) fn visible(&self, height: u16) -> (u16, usize, usize) {
        let count = self
            .options
            .len()
            .min(8)
            .min(height.saturating_sub(4) as usize);
        (
            height.saturating_sub(1 + count as u16),
            (self.selected + 1).saturating_sub(count),
            count,
        )
    }

    pub(super) fn rows(&self, height: u16) -> impl Iterator<Item = (u16, bool, &str)> {
        let (top, offset, count) = self.visible(height);
        self.options
            .iter()
            .enumerate()
            .skip(offset)
            .take(count)
            .map(move |(i, opt)| (top + (i - offset) as u16, i == self.selected, *opt))
    }
}

impl App {
    fn accept_suggestion(&mut self) {
        if let Some(menu) = self.suggestions.take() {
            let word = menu.options[menu.selected];
            let takes_arguments = self.editor.text[..menu.span.start].trim().is_empty()
                && ACTIONS
                    .iter()
                    .flat_map(|group| group.split('|'))
                    .any(|a| a.split_once(' ').is_some_and(|(name, _)| name == word));
            self.editor.cursor = menu.span.end;
            self.editor.anchor = Some(menu.span.start);
            self.editor.replace(word);
            if takes_arguments && self.editor.cursor == self.editor.text.len() {
                self.editor.replace(" ")
            }
        }
    }

    pub(super) fn completion_key(&mut self, k: KeyEvent) -> bool {
        if let Some(menu) = self.suggestions.as_mut() {
            let n = menu.options.len();
            match k.code {
                K::BackTab | K::Up => menu.selected = (menu.selected + n - 1) % n,
                K::Tab if k.modifiers.contains(M::SHIFT) => {
                    menu.selected = (menu.selected + n - 1) % n
                }
                K::Tab | K::Down => menu.selected = (menu.selected + 1) % n,
                K::Enter => self.accept_suggestion(),
                K::Esc => self.suggestions = None,
                _ => {
                    self.suggestions = None;
                    return false;
                }
            }
            return true;
        }
        if !matches!(k.code, K::Tab | K::BackTab) {
            return false;
        }
        let menu = Suggestions::for_editor(&self.editor);
        let count = menu.options.len();
        if count > 0 {
            self.suggestions = Some(menu);
            if count == 1 {
                self.accept_suggestion()
            }
        }
        true
    }

    pub(super) fn completion_mouse(&mut self, m: MouseEvent) -> bool {
        let Some(menu) = self.suggestions.as_mut() else {
            return false;
        };
        let (top, offset, count) = menu.visible(self.height);
        match m.kind {
            MK::Down(B::Left) if (top..top + count as u16).contains(&m.row) => {
                menu.selected = offset + usize::from(m.row - top);
                self.accept_suggestion();
            }
            MK::ScrollUp => menu.selected = menu.selected.saturating_sub(1),
            MK::ScrollDown => menu.selected = (menu.selected + 1).min(menu.options.len() - 1),
            MK::Down(_) => self.suggestions = None,
            _ => (),
        }
        true
    }
}
