// [$$-en-US]#,##0.00_);[Red]([$$-en-US]#,##0.00)

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use crate::formatter::format::format_number;
use crate::formatter::lexer::{Lexer, Token};
use crate::locale::get_default_locale;

#[test]
fn dollar_en_us() {
    let mut lexer = Lexer::new("[$$-en-US]");
    let token = lexer.next_token();
    assert_eq!(token, Token::Currency("$".to_string()));
}

#[test]
fn dollar_409() {
    let mut lexer = Lexer::new("[$$-409]");
    let token = lexer.next_token();
    assert_eq!(token, Token::Currency("$".to_string()));
}

#[test]
fn test_multichar_currency_symbol() {
    // Google Sheets exports "R$" as [$R$ -416]
    let locale = get_default_locale();
    assert_eq!(
        format_number(821941.58, "[$R$ -416]#,##0.00", locale).text,
        "R$ 821,941.58"
    );
    assert_eq!(
        format_number(1.5, "[$R$-416]#,##0.00", locale).text,
        "R$1.50"
    );
}
