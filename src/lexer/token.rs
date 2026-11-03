#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Identifier(String),
    Int(i64),
    Float(f64),
    StringLiteral(String),
    /// Raw content between backticks, still with `${...}` and escape characters. The parser interprets it.
    TemplateString(String),
    Boolean(bool),
    Var,
    Const,
    /// Reserved, like `var` and `const`, for a clear error: Koji declares with `mut` and
    /// `immut`.
    Let,
    Immut,
    Mut,
    Take,
    Type,
    Interface,
    Import,
    Export,
    From,
    If,
    Else,
    For,
    In,
    Is,
    While,
    Func,
    Return,
    Async,
    Await,
    Match,
    Switch,
    Case,
    Default,
    Break,
    Continue,
    Fallthrough,
    And,
    Or,
    Not,
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    Percent,
    Ampersand,
    Pipe,
    Caret,
    Tilde,
    ShiftLeft,
    ShiftRight,
    Equal,
    EqualEqual,
    PlusEqual,
    MinusEqual,
    StarEqual,
    StarStarEqual,
    SlashEqual,
    PercentEqual,
    Bang,
    BangEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Arrow,
    FatArrow,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    /// `..` in a slice: `items[1..3]`.
    DotDot,
    Semicolon,
    Colon,
    /// `?`: error propagation.
    Question,
    /// A character that does not belong to the language.
    Illegal(char),
    EOF,
}

impl Token {
    /// Keyword spelling, so keywords can still be used as member names (`regex.match(...)`).
    pub fn keyword_name(&self) -> Option<&'static str> {
        let name = match self {
            Token::Var => "var",
            Token::Const => "const",
            Token::Let => "let",
            Token::Immut => "immut",
            Token::Mut => "mut",
            Token::Take => "take",
            Token::Type => "type",
            Token::Interface => "interface",
            Token::Import => "import",
            Token::Export => "export",
            Token::From => "from",
            Token::If => "if",
            Token::Else => "else",
            Token::For => "for",
            Token::In => "in",
            Token::Is => "is",
            Token::While => "while",
            Token::Func => "function",
            Token::Return => "return",
            Token::Async => "async",
            Token::Await => "await",
            Token::Match => "match",
            Token::Default => "default",
            Token::Break => "break",
            Token::Continue => "continue",
            Token::Fallthrough => "fallthrough",
            Token::And => "and",
            Token::Or => "or",
            Token::Not => "not",
            _ => return None,
        };
        Some(name)
    }
}

/// A token together with the position (1-based) where it starts.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
    pub column: usize,
}

impl Token {
    /// The text of a symbol (`+=`, `=>`), or `None` for the other tokens.
    pub fn symbol(&self) -> Option<&'static str> {
        Some(match self {
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::StarStar => "**",
            Token::Slash => "/",
            Token::Percent => "%",
            Token::Ampersand => "&",
            Token::Pipe => "|",
            Token::Caret => "^",
            Token::Tilde => "~",
            Token::ShiftLeft => "<<",
            Token::ShiftRight => ">>",
            Token::Equal => "=",
            Token::EqualEqual => "==",
            Token::PlusEqual => "+=",
            Token::MinusEqual => "-=",
            Token::StarEqual => "*=",
            Token::StarStarEqual => "**=",
            Token::SlashEqual => "/=",
            Token::PercentEqual => "%=",
            Token::Bang => "!",
            Token::BangEqual => "!=",
            Token::Greater => ">",
            Token::GreaterEqual => ">=",
            Token::Less => "<",
            Token::LessEqual => "<=",
            Token::Arrow => "->",
            Token::FatArrow => "=>",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::LBracket => "[",
            Token::RBracket => "]",
            Token::Comma => ",",
            Token::Dot => ".",
            Token::DotDot => "..",
            Token::Semicolon => ";",
            Token::Colon => ":",
            Token::Question => "?",
            _ => return None,
        })
    }

    /// The token in the text form of `kojic dump tokens`, shared with the compiler written in
    /// Koji (`compiler/lexer.kj`): `Identifier count`, `Int 42`, `String 'hi'`, `Symbol +=`.
    pub fn dump(&self) -> String {
        if let Some(keyword) = self.keyword_name() {
            return format!("Keyword {}", keyword);
        }
        if let Some(symbol) = self.symbol() {
            return format!("Symbol {}", symbol);
        }
        match self {
            Token::Identifier(name) => format!("Identifier {}", name),
            Token::Int(n) => format!("Int {}", n),
            Token::Float(f) => format!("Float {}", koji_runtime::format_float(*f)),
            Token::StringLiteral(text) => format!("String {}", koji_runtime::repr_string(text)),
            Token::TemplateString(raw) => format!("Template {}", koji_runtime::repr_string(raw)),
            Token::Boolean(b) => format!("Boolean {}", b),
            Token::Illegal(c) => format!("Illegal {}", koji_runtime::repr_string(&c.to_string())),
            Token::EOF => "End".to_string(),
            other => format!("{:?}", other),
        }
    }
}

impl Token {
    /// The token as error messages show it (`'x'`, `'=>'`, `42`, `the end of the file`),
    /// shared with the compiler written in Koji.
    pub fn describe(&self) -> String {
        if let Some(keyword) = self.keyword_name() {
            return format!("'{}'", keyword);
        }
        if let Some(symbol) = self.symbol() {
            return format!("'{}'", symbol);
        }
        match self {
            Token::Identifier(name) => format!("'{}'", name),
            Token::Int(n) => n.to_string(),
            Token::Float(f) => koji_runtime::format_float(*f),
            Token::StringLiteral(text) => koji_runtime::repr_string(text),
            Token::TemplateString(_) => "a template string".to_string(),
            Token::Boolean(b) => b.to_string(),
            Token::Illegal(c) => format!("'{}'", c),
            Token::EOF => "the end of the file".to_string(),
            other => format!("{:?}", other),
        }
    }
}
