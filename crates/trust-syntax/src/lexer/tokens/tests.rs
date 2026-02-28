use super::*;
use logos::Logos;

fn lex(input: &str) -> Vec<(TokenKind, &str)> {
    TokenKind::lexer(input)
        .spanned()
        .map(|(tok, span)| (tok.unwrap_or(TokenKind::Error), &input[span]))
        .collect()
}

#[path = "tests_part_01.rs"]
mod tests_part_01;
#[path = "tests_part_02.rs"]
mod tests_part_02;
