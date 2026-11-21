use super::*;
use ironcalc::base::expressions::{
    lexer::util::get_tokens_with_locale, parser::ExpectedTokens, token::TokenType,
};
impl App {
    pub(super) fn argument_help(&mut self) -> Option<String> {
        if !matches!(self.mode, Mode::Edit) || !self.editor.text.starts_with('=') {
            return None;
        }
        let pos = self.editor.text[..self.editor.cursor].chars().count();
        let (s, v) = self.edit_target();
        let ctx = self
            .book
            .m
            .formula_completion(s, v.r, v.c, &self.editor.text, pos)
            .ok()?;
        ctx.expecting.into_iter().find_map(|item| match item {
            ExpectedTokens::Argument(name, index) => {
                let args = match name.to_ascii_uppercase().as_str() {
                    "IF" | "SE" => "teste; se_verdadeiro; se_falso",
                    "SUMIF" | "AVERAGEIF" => "intervalo; critério; [intervalo_resultado]",
                    "VLOOKUP" | "HLOOKUP" => "chave; tabela; índice; [aproximada]",
                    "XLOOKUP" => "chave; busca; resultados; [ausente]; [modo]; [direção]",
                    "DATE" => "ano; mês; dia",
                    "INDEX" => "matriz; linha; [coluna]",
                    "MATCH" => "chave; intervalo; [tipo]",
                    "SUM" | "SOMA" | "AVERAGE" | "MÉDIA" | "MIN" | "MAX" => {
                        "número1; [número2]; …"
                    }
                    "SUBTOTAL" => "código; intervalo1; …",
                    "IFERROR" => "expressão; alternativa",
                    _ => "",
                };
                Some(format!(
                    "{name}({args})   argumento {index}   F4 selecionar referência"
                ))
            }
            ExpectedTokens::FunctionName(prefix) => Some(
                commands::FUNCTIONS
                    .split_whitespace()
                    .filter(|n| n.starts_with(&prefix.to_uppercase()))
                    .take(6)
                    .collect::<Vec<_>>()
                    .join("  "),
            ),
            _ => None,
        })
    }
    pub(super) fn references(&self) -> Vec<Range> {
        if !matches!(self.mode, Mode::Edit) || !self.editor.text.starts_with('=') {
            return vec![];
        }
        let locale = self.book.m.get_locale();
        let tokens = get_tokens_with_locale(
            &self.editor.text[1..],
            ironcalc::base::locale::get_locale(&locale).unwrap(),
            ironcalc::base::language::get_default_language(),
        );
        let here = &self.book.ws(self.sheet).name;
        let local = self.sheet == self.edit_target().0;
        tokens
            .into_iter()
            .filter_map(|t| match t.token {
                TokenType::Reference {
                    sheet, row, column, ..
                } if sheet.as_ref().map_or(local, |s| s == here) => Some(Range::cell(row, column)),
                TokenType::Range { sheet, left, right }
                    if sheet.as_ref().map_or(local, |s| s == here) =>
                {
                    Some(Range::between(
                        (left.row, left.column),
                        (right.row, right.column),
                    ))
                }
                _ => None,
            })
            .collect()
    }
}
