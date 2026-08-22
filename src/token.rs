use crate::source::Span;

#[derive(Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Integer(String),
    Float(String),
    String(String),
    Keyword(String),
    Special(&'static str),
    Symbol(Symbol),
    Newline,
    Eof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Symbol {
    Assign,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Plus,
    Minus,
    Star,
    Power,
    Slash,
    Percent,
    PlusAssign,
    MinusAssign,
    StarAssign,
    PowerAssign,
    SlashAssign,
    PercentAssign,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Dot,
}

#[must_use]
pub fn is_reserved_word(text: &str) -> bool {
    matches!(
        text,
        "AND"
            | "AS"
            | "BOOLEAN"
            | "BYTE"
            | "CLASS"
            | "CONST"
            | "CONSTRUCTOR"
            | "CONTINUE"
            | "DELETE"
            | "DESTRUCTOR"
            | "DATE"
            | "DIV"
            | "EACH"
            | "ELSE"
            | "END"
            | "EOF"
            | "EXIT"
            | "EXPORT"
            | "EXTENDS"
            | "FALSE"
            | "FLOAT"
            | "FLOAT32"
            | "FLOAT64"
            | "FOR"
            | "FUNCTION"
            | "HOST"
            | "IF"
            | "IMPLEMENTS"
            | "IMPORT"
            | "IN"
            | "INPUT"
            | "INT8"
            | "INT16"
            | "INT32"
            | "INT64"
            | "INTEGER"
            | "INTERFACE"
            | "IS"
            | "LET"
            | "NA"
            | "NEW"
            | "NOT"
            | "NULL"
            | "OR"
            | "PARALLEL"
            | "POINTER"
            | "PRIVATE"
            | "PRINT"
            | "PUBLIC"
            | "REPEAT"
            | "RETURN"
            | "SELF"
            | "SHL"
            | "SHR"
            | "STATIC"
            | "STEP"
            | "STOP"
            | "STRING"
            | "STRUCT"
            | "SYSTEM"
            | "THEN"
            | "TIME"
            | "TIMESTAMP"
            | "TIMEZONE"
            | "TO"
            | "TRUE"
            | "UINT16"
            | "UINT32"
            | "UINT64"
            | "UNTIL"
            | "VOID"
            | "WHILE"
            | "XOR"
    )
}
