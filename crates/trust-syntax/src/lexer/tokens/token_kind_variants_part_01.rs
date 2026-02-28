    // =========================================================================
    // TRIVIA
    // =========================================================================
    /// Whitespace (spaces, tabs, newlines)
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    /// Single-line comment: // ...
    #[regex(r"//[^\r\n]*", allow_greedy = true)]
    LineComment,

    /// Block comment: (* ... *) or /* ... */ (supports nesting).
    #[token("(*", lex_block_comment_pascal)]
    #[token("/*", lex_block_comment_c)]
    BlockComment,

    /// Pragma: { ... }
    /// IEC 61131-3 Section 6.2, Table 4
    /// Pragma contents are implementer-specific; treated as trivia
    #[regex(r"\{[^}]*\}")]
    Pragma,

    // =========================================================================
    // PUNCTUATION
    // =========================================================================
    /// `;`
    #[token(";")]
    Semicolon,

    /// `:`
    #[token(":")]
    Colon,

    /// `,`
    #[token(",")]
    Comma,

    /// `.`
    #[token(".")]
    Dot,

    /// `..`
    #[token("..")]
    DotDot,

    /// `(`
    #[token("(")]
    LParen,

    /// `)`
    #[token(")")]
    RParen,

    /// `[`
    #[token("[")]
    LBracket,

    /// `]`
    #[token("]")]
    RBracket,

    /// `#`
    #[token("#")]
    Hash,

    /// `^`
    #[token("^")]
    Caret,

    /// `@`
    #[token("@")]
    At,

    // =========================================================================
    // OPERATORS - Assignment
    // =========================================================================
    /// `:=`
    #[token(":=")]
    Assign,

    /// `=>`
    #[token("=>")]
    Arrow,

    /// `?=`
    #[token("?=")]
    RefAssign,

    // =========================================================================
    // OPERATORS - Comparison
    // =========================================================================
    /// `=`
    #[token("=")]
    Eq,

    /// `<>`
    #[token("<>")]
    Neq,

    /// `<`
    #[token("<")]
    Lt,

    /// `<=`
    #[token("<=")]
    LtEq,

    /// `>`
    #[token(">")]
    Gt,

    /// `>=`
    #[token(">=")]
    GtEq,

    // =========================================================================
    // OPERATORS - Arithmetic
    // =========================================================================
    /// `+`
    #[token("+")]
    Plus,

    /// `-`
    #[token("-")]
    Minus,

    /// `*`
    #[token("*")]
    Star,

    /// `/`
    #[token("/")]
    Slash,

    /// `**`
    #[token("**")]
    Power,

    /// `&`
    #[token("&")]
    Ampersand,

    // =========================================================================
    // KEYWORDS - Program Organization Units
    // =========================================================================
    /// `PROGRAM`
    #[token("PROGRAM", ignore(case))]
    KwProgram,

    /// `END_PROGRAM`
    #[token("END_PROGRAM", ignore(case))]
    KwEndProgram,

    /// `TEST_PROGRAM` (non-IEC extension for user tests)
    #[token("TEST_PROGRAM", ignore(case))]
    KwTestProgram,

    /// `END_TEST_PROGRAM`
    #[token("END_TEST_PROGRAM", ignore(case))]
    KwEndTestProgram,

    /// `FUNCTION`
    #[token("FUNCTION", ignore(case))]
    KwFunction,

    /// `END_FUNCTION`
    #[token("END_FUNCTION", ignore(case))]
    KwEndFunction,

    /// `FUNCTION_BLOCK`
    #[token("FUNCTION_BLOCK", ignore(case))]
    KwFunctionBlock,

    /// `END_FUNCTION_BLOCK`
    #[token("END_FUNCTION_BLOCK", ignore(case))]
    KwEndFunctionBlock,

    /// `TEST_FUNCTION_BLOCK` (non-IEC extension for user tests)
    #[token("TEST_FUNCTION_BLOCK", ignore(case))]
    KwTestFunctionBlock,

    /// `END_TEST_FUNCTION_BLOCK`
    #[token("END_TEST_FUNCTION_BLOCK", ignore(case))]
    KwEndTestFunctionBlock,

    /// `CLASS`
    #[token("CLASS", ignore(case))]
    KwClass,

    /// `END_CLASS`
    #[token("END_CLASS", ignore(case))]
    KwEndClass,

    /// `METHOD`
    #[token("METHOD", ignore(case))]
    KwMethod,

    /// `END_METHOD`
    #[token("END_METHOD", ignore(case))]
    KwEndMethod,

    /// `PROPERTY`
    #[token("PROPERTY", ignore(case))]
    KwProperty,

    /// `END_PROPERTY`
    #[token("END_PROPERTY", ignore(case))]
    KwEndProperty,

    /// `INTERFACE`
    #[token("INTERFACE", ignore(case))]
    KwInterface,

    /// `END_INTERFACE`
    #[token("END_INTERFACE", ignore(case))]
    KwEndInterface,

    /// `NAMESPACE`
    #[token("NAMESPACE", ignore(case))]
    KwNamespace,

    /// `END_NAMESPACE`
    #[token("END_NAMESPACE", ignore(case))]
    KwEndNamespace,

    /// `USING`
    #[token("USING", ignore(case))]
    KwUsing,

    /// `ACTION`
    #[token("ACTION", ignore(case))]
    KwAction,

    /// `END_ACTION`
    #[token("END_ACTION", ignore(case))]
    KwEndAction,

