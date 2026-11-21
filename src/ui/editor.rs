use crossterm::event::{KeyCode as K, KeyEvent, KeyModifiers as M};
/// Cursor and anchor are UTF-8 byte boundaries. All mutation goes through replace().
#[derive(Default)]
pub struct Editor {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
}
impl Editor {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.into(),
            cursor: text.len(),
            anchor: None,
        }
    }
    pub fn replace(&mut self, s: &str) {
        let a = self.anchor.take().unwrap_or(self.cursor);
        let lo = a.min(self.cursor);
        self.text.replace_range(lo..a.max(self.cursor), s);
        self.cursor = lo + s.len()
    }
    fn move_to(&mut self, n: usize, shift: bool) {
        if shift {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None
        }
        self.cursor = n
    }
    pub fn key(&mut self, k: KeyEvent) {
        let left = self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
        let right = self.cursor
            + self.text[self.cursor..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
        let shift = k.modifiers.contains(M::SHIFT);
        match k.code {
            K::Left => self.move_to(left, shift),
            K::Right => self.move_to(right, shift),
            K::Home => self.move_to(0, shift),
            K::End => self.move_to(self.text.len(), shift),
            K::Char('a') if k.modifiers.contains(M::CONTROL) => {
                self.anchor = Some(0);
                self.cursor = self.text.len()
            }
            K::Delete | K::Backspace => {
                if self.anchor.is_none() {
                    self.anchor = Some(if k.code == K::Delete { right } else { left })
                }
                self.replace("")
            }
            K::Char(c) if !k.modifiers.intersects(M::CONTROL | M::ALT) && !c.is_control() => {
                self.replace(&c.to_string())
            }
            K::Enter if k.modifiers.contains(M::ALT) => self.replace("\n"),
            _ => (),
        }
    }
}
