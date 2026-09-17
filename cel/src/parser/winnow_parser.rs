//! A CEL parser built on the [`winnow`] parser-combinator crate.
//!
//! The parser is split in two winnow passes:
//!
//! 1. a lexer over the source text ([`lex`]) that mirrors the lexer rules
//!    of `CEL.g4` and produces a flat `Vec<Token>` of byte-span tokens, and
//! 2. a recursive-descent parser over a [`TokenSlice`] of those tokens that
//!    mirrors the grammar rules of `CEL.g4` one function per rule (`expr`,
//!    `conditionalOr`, `conditionalAnd`, `relation`, `calc`, `unary`,
//!    `member`, `primary`).
//!
//! It produces the very same [`IdedExpr`] tree — same shape, same node ids,
//! same [`SourceInfo`] offsets — as the default ANTLR-generated parser in
//! [`crate::parser::Parser`], so the two can be swapped freely. Error
//! *messages* are this parser's own, but errors are reported at the same
//! positions, and the `max_recursion_depth` / `error_recovery_limit` knobs
//! follow the same semantics.
//!
//! Parser state (id counter, source info, options, collected errors) is
//! threaded through the token stream with [`Stateful`]. Syntax errors are
//! raised as [`ErrMode::Cut`] and caught at recovery points (list elements,
//! map / struct entries, call arguments and the top level), where they are
//! recorded and the parser re-synchronises on the next `,` `)` `]` `}`.

use crate::common::ast::{
    operators, CallExpr, EntryExpr, Expr, IdedEntryExpr, IdedExpr, ListExpr, LiteralValue,
    MapEntryExpr, MapExpr, SelectExpr, SourceInfo, StructExpr, StructFieldExpr,
};
use crate::parser::{macros, parse, MacroExprHelper, ParseError, ParseErrors, ParserHelper};
use std::fmt;
use std::mem;
use std::sync::Arc;
use winnow::combinator::{alt, dispatch, fail, opt, peek, repeat, repeat_till};
use winnow::error::{ErrMode, ModalResult, ParserError};
use winnow::stream::{
    AsChar, ContainsToken, LocatingSlice, Location, Stateful, Stream, TokenSlice,
};
use winnow::token::{any, none_of, one_of, rest, take_till, take_while};
use winnow::Parser;

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

/// The kinds of token produced by [`lex`], one per lexer rule of `CEL.g4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    // Literals
    Int,
    Uint,
    Float,
    String,
    Bytes,
    // Identifiers and keywords
    Ident,
    EscIdent,
    True,
    False,
    Null,
    In,
    // Delimiters
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Dot,
    Comma,
    Question,
    Colon,
    // Operators
    Minus,
    Plus,
    Star,
    Slash,
    Percent,
    Exclam,
    EqEq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    And,
    Or,
    /// A stretch of source the lexer could not tokenize. The lexer has
    /// already reported it; the parser silently treats it as a placeholder
    /// expression so the same stretch isn't reported twice.
    Error,
}

impl TokenKind {
    fn as_str(self) -> &'static str {
        match self {
            TokenKind::Int => "int",
            TokenKind::Uint => "uint",
            TokenKind::Float => "float",
            TokenKind::String => "string",
            TokenKind::Bytes => "bytes",
            TokenKind::Ident => "identifier",
            TokenKind::EscIdent => "identifier",
            TokenKind::True => "'true'",
            TokenKind::False => "'false'",
            TokenKind::Null => "'null'",
            TokenKind::In => "'in'",
            TokenKind::LParen => "'('",
            TokenKind::RParen => "')'",
            TokenKind::LBracket => "'['",
            TokenKind::RBracket => "']'",
            TokenKind::LBrace => "'{'",
            TokenKind::RBrace => "'}'",
            TokenKind::Dot => "'.'",
            TokenKind::Comma => "','",
            TokenKind::Question => "'?'",
            TokenKind::Colon => "':'",
            TokenKind::Minus => "'-'",
            TokenKind::Plus => "'+'",
            TokenKind::Star => "'*'",
            TokenKind::Slash => "'/'",
            TokenKind::Percent => "'%'",
            TokenKind::Exclam => "'!'",
            TokenKind::EqEq => "'=='",
            TokenKind::NotEq => "'!='",
            TokenKind::Less => "'<'",
            TokenKind::LessEq => "'<='",
            TokenKind::Greater => "'>'",
            TokenKind::GreaterEq => "'>='",
            TokenKind::And => "'&&'",
            TokenKind::Or => "'||'",
            TokenKind::Error => "error",
        }
    }
}

/// A lexed token: its kind and the `start..end` byte span in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

// Lets a `TokenKind` be used as a winnow parser / token set over the token
// stream (`literal(TokenKind::Comma)`, `one_of(TokenKind::Comma)`).
impl PartialEq<TokenKind> for Token {
    fn eq(&self, other: &TokenKind) -> bool {
        self.kind == *other
    }
}

impl ContainsToken<&Token> for TokenKind {
    #[inline(always)]
    fn contains_token(&self, token: &Token) -> bool {
        token.kind == *self
    }
}

impl<const N: usize> ContainsToken<&Token> for [TokenKind; N] {
    #[inline]
    fn contains_token(&self, token: &Token) -> bool {
        self.contains(&token.kind)
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

type LexInput<'s> = LocatingSlice<&'s str>;

/// Lexer-level failure; the message is what gets reported to the user.
#[derive(Debug)]
struct LexFailure(&'static str);

impl<I: Stream> ParserError<I> for LexFailure {
    type Inner = Self;

    fn from_input(_: &I) -> Self {
        LexFailure("unexpected character")
    }

    fn into_inner(self) -> Result<Self::Inner, Self> {
        Ok(self)
    }
}

type LexResult<O> = ModalResult<O, LexFailure>;

fn lex_fail<O>(msg: &'static str) -> LexResult<O> {
    Err(ErrMode::Cut(LexFailure(msg)))
}

/// A lexer error, at a byte offset in the source.
#[derive(Debug, PartialEq, Eq)]
pub struct LexError {
    pub offset: usize,
    pub message: &'static str,
}

/// Tokenize `source`. Whitespace and `//` comments are dropped. Every
/// stretch that fails to lex yields both a [`LexError`] and a
/// [`TokenKind::Error`] token spanning it, and lexing carries on.
pub fn lex(source: &str) -> (Vec<Token>, Vec<LexError>) {
    let mut input = LocatingSlice::new(source);
    let mut tokens = Vec::with_capacity(source.len() / 4 + 4);
    let mut errors = Vec::new();
    loop {
        // Whitespace and comments never fail; ignore the (unit) result.
        let _ = trivia.parse_next(&mut input);
        if input.is_empty() {
            break;
        }
        let start = Location::current_token_start(&input);
        match token.parse_next(&mut input) {
            Ok(kind) => {
                let end = Location::current_token_start(&input);
                tokens.push(Token { kind, start, end });
            }
            Err(err) => {
                let message = err
                    .into_inner()
                    .map(|LexFailure(msg)| msg)
                    .unwrap_or("unexpected character");
                if Location::current_token_start(&input) == start {
                    // Nothing consumed: skip the offending character so we
                    // always make progress.
                    let _ = any::<_, LexFailure>.parse_next(&mut input);
                }
                let end = Location::current_token_start(&input);
                errors.push(LexError {
                    offset: start,
                    message,
                });
                tokens.push(Token {
                    kind: TokenKind::Error,
                    start,
                    end,
                });
            }
        }
    }
    (tokens, errors)
}

/// `WHITESPACE` and `COMMENT` — both on the hidden channel in the grammar.
fn trivia(i: &mut LexInput<'_>) -> LexResult<()> {
    repeat(
        0..,
        alt((
            take_while(1.., [' ', '\t', '\r', '\n', '\u{0C}']).void(),
            ("//", take_till(0.., '\n')).void(),
        )),
    )
    .parse_next(i)
}

fn token(i: &mut LexInput<'_>) -> LexResult<TokenKind> {
    use TokenKind::*;
    dispatch! {peek(any);
        '0'..='9' => number,
        '.' => alt((number, '.'.value(Dot))),
        '"' | '\'' => quoted(false, false).value(String),
        'r' | 'R' => alt((
            (one_of(['r', 'R']), quoted(true, false)).value(String),
            ident_or_keyword,
        )),
        'b' | 'B' => alt((
            (one_of(['b', 'B']), bytes_body).value(Bytes),
            ident_or_keyword,
        )),
        'a'..='z' | 'A'..='Z' | '_' => ident_or_keyword,
        '`' => esc_ident,
        '(' => '('.value(LParen),
        ')' => ')'.value(RParen),
        '[' => '['.value(LBracket),
        ']' => ']'.value(RBracket),
        '{' => '{'.value(LBrace),
        '}' => '}'.value(RBrace),
        ',' => ','.value(Comma),
        '?' => '?'.value(Question),
        ':' => ':'.value(Colon),
        '-' => '-'.value(Minus),
        '+' => '+'.value(Plus),
        '*' => '*'.value(Star),
        '/' => '/'.value(Slash),
        '%' => '%'.value(Percent),
        '!' => alt(("!=".value(NotEq), '!'.value(Exclam))),
        '=' => "==".value(EqEq),
        '<' => alt(("<=".value(LessEq), '<'.value(Less))),
        '>' => alt((">=".value(GreaterEq), '>'.value(Greater))),
        '&' => doubled('&', And, "unexpected single '&', expected '&&'"),
        '|' => doubled('|', Or, "unexpected single '|', expected '||'"),
        _ => fail,
    }
    .parse_next(i)
}

/// `&&` / `||`: a lone `&` or `|` is consumed and reported with `msg`.
fn doubled(
    mut c: char,
    kind: TokenKind,
    msg: &'static str,
) -> impl FnMut(&mut LexInput<'_>) -> LexResult<TokenKind> {
    move |i| {
        c.parse_next(i)?;
        if opt(c).parse_next(i)?.is_some() {
            Ok(kind)
        } else {
            lex_fail(msg)
        }
    }
}

fn digits<'s>(i: &mut LexInput<'s>) -> LexResult<&'s str> {
    take_while(1.., AsChar::is_dec_digit).parse_next(i)
}

/// `EXPONENT : ('e' | 'E') ('+' | '-')? DIGIT+`
fn exponent(i: &mut LexInput<'_>) -> LexResult<()> {
    (one_of(['e', 'E']), opt(one_of(['+', '-'])), digits)
        .void()
        .parse_next(i)
}

/// `NUM_INT`, `NUM_UINT` and `NUM_FLOAT`, longest match first, exactly as
/// the ANTLR lexer resolves them (so `1.` is an int followed by a dot, `0x`
/// without hex digits is `0` followed by the identifier `x`, and only a
/// lowercase `x` introduces a hex literal).
fn number(i: &mut LexInput<'_>) -> LexResult<TokenKind> {
    if opt(("0x", take_while(1.., AsChar::is_hex_digit)))
        .parse_next(i)?
        .is_some()
    {
        return integral_suffix(i);
    }
    let int_part = opt(digits).parse_next(i)?;
    let mut is_float = opt(('.', digits)).parse_next(i)?.is_some();
    if int_part.is_none() && !is_float {
        return fail.parse_next(i);
    }
    if opt(exponent).parse_next(i)?.is_some() {
        is_float = true;
    }
    if is_float {
        Ok(TokenKind::Float)
    } else {
        integral_suffix(i)
    }
}

fn integral_suffix(i: &mut LexInput<'_>) -> LexResult<TokenKind> {
    Ok(if opt(one_of(['u', 'U'])).parse_next(i)?.is_some() {
        TokenKind::Uint
    } else {
        TokenKind::Int
    })
}

/// `IDENTIFIER`, with the grammar keywords (`true`, `false`, `null`, `in`)
/// split out. Reserved words (`as`, `while`, …) are plain identifiers here,
/// exactly like in `CEL.g4`; the parser rejects them where the grammar does.
fn ident_or_keyword(i: &mut LexInput<'_>) -> LexResult<TokenKind> {
    let word = (
        one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
    )
        .take()
        .parse_next(i)?;
    Ok(match word {
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "null" => TokenKind::Null,
        "in" => TokenKind::In,
        _ => TokenKind::Ident,
    })
}

/// `` ESC_IDENTIFIER : '`' (LETTER | DIGIT | '_' | '.' | '-' | '/' | ' ')+ '`' ``
fn esc_ident(i: &mut LexInput<'_>) -> LexResult<TokenKind> {
    '`'.parse_next(i)?;
    let body = take_while(0.., |c: char| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/' | ' ')
    })
    .parse_next(i)?;
    match opt('`').parse_next(i)? {
        Some(_) if !body.is_empty() => Ok(TokenKind::EscIdent),
        Some(_) => lex_fail("invalid quoted identifier"),
        None if opt((take_till(0.., '`'), '`')).parse_next(i)?.is_some() => {
            lex_fail("invalid quoted identifier")
        }
        None => {
            rest.void().parse_next(i)?;
            lex_fail("unterminated quoted identifier")
        }
    }
}

/// The `STRING` part of a `BYTES` token: `('b' | 'B')` has been consumed.
fn bytes_body(i: &mut LexInput<'_>) -> LexResult<()> {
    let raw = opt(one_of(['r', 'R'])).parse_next(i)?.is_some();
    quoted(raw, true).parse_next(i)
}

/// A quoted literal starting at its opening quote: single or triple quoted,
/// with `\`-escapes unless `raw`. The escape sequences themselves are only
/// validated later, by [`parse::parse_string`] / [`parse::parse_bytes`].
///
/// An unterminated literal swallows the rest of the input and fails with
/// a `Cut`, so the caller reports it and the lexer moves on to the end.
fn quoted(raw: bool, is_bytes: bool) -> impl FnMut(&mut LexInput<'_>) -> LexResult<()> {
    move |i| {
        let quote: char = one_of(['"', '\'']).parse_next(i)?;
        let triple = opt(if quote == '"' { "\"\"" } else { "''" })
            .parse_next(i)?
            .is_some();
        let closing = match quote {
            '"' if triple => "\"\"\"",
            '"' => "\"",
            _ if triple => "'''",
            _ => "'",
        };
        let body: LexResult<()> = match (raw, triple) {
            (true, true) => repeat_till(0.., any, closing)
                .map(|((), _)| ())
                .parse_next(i),
            (false, true) => repeat_till(0.., alt((('\\', any).void(), any.void())), closing)
                .map(|((), _)| ())
                .parse_next(i),
            (true, false) => (take_till(0.., [quote, '\n', '\r']), closing)
                .void()
                .parse_next(i),
            (false, false) => (
                repeat(
                    0..,
                    alt((
                        take_till(1.., [quote, '\\', '\n', '\r']).void(),
                        ('\\', none_of(['\n', '\r'])).void(),
                    )),
                )
                .map(|()| ()),
                closing,
            )
                .void()
                .parse_next(i),
        };
        match body {
            Ok(()) => Ok(()),
            Err(ErrMode::Backtrack(_)) => {
                rest.void().parse_next(i)?;
                lex_fail(if is_bytes {
                    "unterminated bytes literal"
                } else {
                    "unterminated string literal"
                })
            }
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Parser: configuration, state and errors
// ---------------------------------------------------------------------------

/// A CEL parser built on winnow. The options and their defaults match
/// [`crate::parser::Parser`].
#[derive(Debug, Clone, Copy)]
pub struct WinnowParser {
    max_recursion_depth: u16,
    error_recovery_limit: u32,
    enable_optional_syntax: bool,
    enable_ident_escape_syntax: bool,
}

impl Default for WinnowParser {
    fn default() -> Self {
        Self::new()
    }
}

impl WinnowParser {
    pub fn new() -> Self {
        Self {
            max_recursion_depth: 96,
            error_recovery_limit: 30,
            enable_optional_syntax: false,
            enable_ident_escape_syntax: false,
        }
    }

    /// Sets how deeply `expr` rules may nest: `max` allows `max + 1` nested
    /// expressions, exactly like [`crate::parser::Parser::max_recursion_depth`].
    pub fn max_recursion_depth(mut self, max: u16) -> Self {
        self.max_recursion_depth = max;
        self
    }

    /// Sets the number of syntax errors the parser recovers from before it
    /// gives up with `error recovery attempt limit exceeded`.
    pub fn error_recovery_limit(mut self, limit: u32) -> Self {
        self.error_recovery_limit = limit;
        self
    }

    /// Enables the optional syntax: `a.?b`, `a[?b]`, `[?a]`, `{?k: v}` and
    /// `Msg{?f: v}`.
    pub fn enable_optional_syntax(mut self, enable: bool) -> Self {
        self.enable_optional_syntax = enable;
        self
    }

    /// Enables backtick-escaped field identifiers (``` a.`b-c` ```).
    pub fn enable_ident_escape_syntax(mut self, enable: bool) -> Self {
        self.enable_ident_escape_syntax = enable;
        self
    }

    /// Parse, discarding the source info. See [`Self::parse_with_source_info`].
    pub fn parse(self, source: &str) -> Result<IdedExpr, ParseErrors> {
        self.parse_with_source_info(source).map(|(expr, _)| expr)
    }

    /// Parse, returning the source info alongside the expression.
    pub fn parse_with_source_info(
        self,
        source: &str,
    ) -> Result<(IdedExpr, SourceInfo), ParseErrors> {
        let (tokens, lex_errors) = lex(source);

        let mut helper = ParserHelper::default();
        helper.source_info.source = source.to_string();
        let mut state = State {
            source,
            options: self,
            helper,
            errors: Vec::new(),
            depth: 0,
            recoveries: 0,
            aborted: false,
        };
        for err in lex_errors {
            state.push_error(err.offset, err.message.to_string());
            if state.count_recovery().is_err() {
                break;
            }
        }

        let mut input = Input {
            input: TokenSlice::new(&tokens),
            state,
        };
        let expr = if input.state.aborted {
            IdedExpr::default()
        } else {
            let expr = recover(&mut input, expr).unwrap_or_default();
            if !input.state.aborted && !input.is_empty() {
                let _ = recover_here(&mut input, "<EOF>");
            }
            expr
        };

        let State {
            mut errors, helper, ..
        } = input.state;
        let source_info = helper.source_info;
        if errors.is_empty() {
            return Ok((expr, source_info));
        }
        errors.sort_by_key(|e| e.pos);
        let source_info = Arc::new(source_info);
        for err in &mut errors {
            err.source_info = Some(source_info.clone());
        }
        Err(ParseErrors { errors })
    }
}

/// Parser state, threaded through the token stream by [`Stateful`].
struct State<'s> {
    source: &'s str,
    options: WinnowParser,
    helper: ParserHelper,
    errors: Vec<ParseError>,
    /// Number of `expr` rules currently being parsed.
    depth: u32,
    /// Number of errors recovered from so far.
    recoveries: u32,
    /// Set once a limit is exceeded: every rule then unwinds immediately.
    aborted: bool,
}

impl fmt::Debug for State<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("depth", &self.depth)
            .field("recoveries", &self.recoveries)
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
}

impl<'s> State<'s> {
    fn text(&self, tok: &Token) -> &'s str {
        &self.source[tok.start..tok.end]
    }

    fn push_error(&mut self, offset: usize, msg: String) {
        let pos = pos_for_offset(self.source, offset);
        self.errors.push(ParseError {
            source: None,
            pos,
            msg,
            expr_id: 0,
            source_info: None,
        });
    }

    /// Reports an error that doesn't affect the parse (unsupported syntax,
    /// reserved identifiers, malformed literals). Not a recovery.
    fn report_at(&mut self, tok: &Token, msg: impl Into<String>) {
        self.push_error(tok.start, msg.into());
    }

    /// Reports a `mismatched input … expecting …` syntax error at `tok`
    /// (`None` is end of input) and counts the recovery. Errors at a lexer
    /// [`TokenKind::Error`] token are not reported again: the lexer already
    /// did.
    fn recover_from(&mut self, tok: Option<Token>, expecting: &'static str) -> PResult<()> {
        match tok {
            Some(tok) if tok.kind == TokenKind::Error => {}
            Some(tok) => {
                let msg = mismatched(self.source, Some(tok), expecting);
                self.push_error(tok.start, msg);
            }
            None => {
                let msg = mismatched(self.source, None, expecting);
                self.push_error(self.source.len(), msg);
            }
        }
        self.count_recovery()
    }

    fn count_recovery(&mut self) -> PResult<()> {
        self.recoveries += 1;
        if self.recoveries > self.options.error_recovery_limit {
            Err(self.abort(format!(
                "error recovery attempt limit exceeded: {}",
                self.options.error_recovery_limit
            )))
        } else {
            Ok(())
        }
    }

    /// Records `msg` and flags the parse as aborted; returns the error every
    /// rule must now unwind with.
    fn abort(&mut self, msg: String) -> ErrMode<PErr> {
        self.aborted = true;
        self.errors.push(ParseError {
            source: None,
            pos: (0, 0),
            msg,
            expr_id: 0,
            source_info: None,
        });
        ErrMode::Cut(PErr::Abort)
    }

    /// Allocates the next id, spanning `tok` (with an inclusive end offset,
    /// like the ANTLR token `stop`).
    fn next_id(&mut self, tok: &Token) -> u64 {
        let id = self.reserve_id();
        self.set_offset(id, tok);
        id
    }

    /// Allocates the next id without a span; see [`Self::set_offset`].
    fn reserve_id(&mut self) -> u64 {
        let id = self.helper.next_id;
        self.helper.next_id += 1;
        id
    }

    fn set_offset(&mut self, id: u64, tok: &Token) {
        self.helper
            .source_info
            .add_offset(id, tok.start as u32, tok.end.saturating_sub(1) as u32);
    }

    fn next_expr(&mut self, tok: &Token, expr: Expr) -> IdedExpr {
        IdedExpr {
            id: self.next_id(tok),
            expr,
        }
    }
}

type Input<'a> = Stateful<TokenSlice<'a, Token>, State<'a>>;

/// The parser's winnow error type.
///
/// Deliberately allocation-free: every optional-token probe that misses
/// (`opt(one_of(..))`) builds one of these, so the message is only
/// rendered by the recovery point that ends up reporting it.
#[derive(Debug)]
enum PErr {
    /// A `mismatched input … expecting …` syntax error at `tok` (`None` at
    /// end of input), to be recorded by whichever recovery point catches it.
    Syntax {
        tok: Option<Token>,
        expecting: &'static str,
    },
    /// A limit was exceeded and recorded; unwind without recording more.
    Abort,
}

impl<'a> ParserError<Input<'a>> for PErr {
    type Inner = Self;

    fn from_input(input: &Input<'a>) -> Self {
        syntax_err(input, "expression")
    }

    fn into_inner(self) -> Result<Self::Inner, Self> {
        Ok(self)
    }
}

type PResult<O> = ModalResult<O, PErr>;

fn quoted_text(source: &str, tok: Option<Token>) -> String {
    match tok {
        None => "'<EOF>'".to_string(),
        Some(tok) => format!("'{}'", &source[tok.start..tok.end]),
    }
}

fn mismatched(source: &str, tok: Option<Token>, expecting: &str) -> String {
    format!(
        "Syntax error: mismatched input {} expecting {expecting}",
        quoted_text(source, tok)
    )
}

/// A `mismatched input … expecting …` error at the current token.
fn syntax_err(i: &Input<'_>, expecting: &'static str) -> PErr {
    PErr::Syntax {
        tok: i.first().copied(),
        expecting,
    }
}

/// Converts a byte offset into the 1-based `(line, column)` pair used by
/// [`ParseError`].
fn pos_for_offset(source: &str, offset: usize) -> (isize, isize) {
    let offset = offset.min(source.len());
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |nl| nl + 1);
    (line as isize, (offset - line_start + 1) as isize)
}

/// Reserved words that the lexer emits as plain identifiers but that may
/// not be used as identifiers or function names (cel-go's `reservedIds`).
fn is_reserved_id(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "else"
            | "for"
            | "function"
            | "if"
            | "import"
            | "let"
            | "loop"
            | "package"
            | "namespace"
            | "return"
            | "var"
            | "void"
            | "while"
    )
}

// ---------------------------------------------------------------------------
// Parser: token stream helpers and error recovery
// ---------------------------------------------------------------------------

fn peek_kind(i: &Input<'_>) -> Option<TokenKind> {
    i.first().map(|t| t.kind)
}

fn kind_at(i: &Input<'_>, n: usize) -> Option<TokenKind> {
    i.get(n).map(|t| t.kind)
}

/// Matches (and copies out) one token of the given kind.
fn tok<'a>(kind: TokenKind) -> impl Parser<Input<'a>, Token, ErrMode<PErr>> {
    one_of(kind).map(|t: &Token| *t)
}

/// Consumes the next token if it is of the given kind.
fn eat(i: &mut Input<'_>, kind: TokenKind) -> Option<Token> {
    opt(tok(kind)).parse_next(i).ok().flatten()
}

/// Requires the next token to be of the given kind; a mismatch is a `Cut`
/// that unwinds to the nearest recovery point.
fn expect(i: &mut Input<'_>, kind: TokenKind) -> PResult<Token> {
    eat(i, kind).ok_or_else(|| ErrMode::Cut(syntax_err(i, kind.as_str())))
}

/// Tokens the parser re-synchronises on after an error.
fn is_sync(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Comma | TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace
    )
}

/// Records a syntax error at the current token, without unwinding.
fn recover_here(i: &mut Input<'_>, expecting: &'static str) -> PResult<()> {
    let tok = i.first().copied();
    i.state.recover_from(tok, expecting)
}

/// A closing delimiter. When it is missing, the error is recorded and the
/// parser skips ahead to the next delimiter, consuming it if it is the one
/// that was expected — so a construct with a botched tail still yields a
/// node and the enclosing rule carries on.
fn expect_close(i: &mut Input<'_>, kind: TokenKind) -> PResult<()> {
    if eat(i, kind).is_some() {
        return Ok(());
    }
    recover_here(i, kind.as_str())?;
    while let Some(k) = peek_kind(i) {
        if is_sync(k) {
            if k == kind {
                i.next_token();
            }
            break;
        }
        i.next_token();
    }
    Ok(())
}

/// A recovery point: runs `parser`, and on a syntax error records it,
/// skips ahead to the next delimiter (not consumed) and yields a default
/// value so the enclosing rule can go on. Aborts are not recovered from.
fn recover<'a, O: Default>(
    i: &mut Input<'a>,
    parser: impl FnOnce(&mut Input<'a>) -> PResult<O>,
) -> PResult<O> {
    match parser(i) {
        Ok(o) => Ok(o),
        Err(ErrMode::Backtrack(PErr::Syntax { tok, expecting }))
        | Err(ErrMode::Cut(PErr::Syntax { tok, expecting })) => {
            i.state.recover_from(tok, expecting)?;
            while let Some(k) = peek_kind(i) {
                if is_sync(k) {
                    break;
                }
                i.next_token();
            }
            Ok(O::default())
        }
        Err(e) => Err(e),
    }
}

fn call(id: u64, func_name: &str, args: Vec<IdedExpr>) -> IdedExpr {
    IdedExpr {
        id,
        expr: Expr::Call(CallExpr {
            func_name: func_name.to_string(),
            target: None,
            args,
        }),
    }
}

/// Builds a call, or expands it when `func_name` (with this target/arity)
/// is a macro.
fn call_or_macro(
    i: &mut Input<'_>,
    id: u64,
    func_name: String,
    target: Option<IdedExpr>,
    args: Vec<IdedExpr>,
) -> IdedExpr {
    match macros::find_expander(&func_name, target.as_ref(), &args) {
        None => IdedExpr {
            id,
            expr: Expr::Call(CallExpr {
                func_name,
                target: target.map(Box::new),
                args,
            }),
        },
        Some(expander) => {
            let mut helper = MacroExprHelper {
                helper: &mut i.state.helper,
                id,
            };
            match expander(&mut helper, target, args) {
                Ok(expr) => expr,
                Err(err) => {
                    i.state.errors.push(err);
                    IdedExpr::default()
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Parser: grammar rules
// ---------------------------------------------------------------------------

/// `expr : conditionalOr ('?' conditionalOr ':' expr)?`
///
/// Also where nesting depth is enforced, as `expr` is the rule the ANTLR
/// parser's recursion listener counts.
fn expr<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    if i.state.aborted {
        return Err(ErrMode::Cut(PErr::Abort));
    }
    let max = i.state.options.max_recursion_depth;
    if i.state.depth > u32::from(max) {
        return Err(i
            .state
            .abort(format!("expression recursion limit exceeded: {max}")));
    }
    i.state.depth += 1;
    let result = conditional(i);
    i.state.depth -= 1;
    result
}

fn conditional<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let condition = conditional_or(i)?;
    let Some(question) = eat(i, TokenKind::Question) else {
        return Ok(condition);
    };
    let op_id = i.state.next_id(&question);
    let if_true = conditional_or(i)?;
    expect(i, TokenKind::Colon)?;
    let if_false = expr(i)?;
    Ok(call(
        op_id,
        operators::CONDITIONAL,
        vec![condition, if_true, if_false],
    ))
}

/// `conditionalOr : conditionalAnd ('||' conditionalAnd)*`
fn conditional_or<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    logical_chain(i, TokenKind::Or, operators::LOGICAL_OR, conditional_and)
}

/// `conditionalAnd : relation ('&&' relation)*`
fn conditional_and<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    logical_chain(i, TokenKind::And, operators::LOGICAL_AND, relation)
}

/// A run of the same logical operator, folded into a balanced tree. Each
/// operator is numbered *after* its right operand, as the ANTLR visitor does.
fn logical_chain<'a>(
    i: &mut Input<'a>,
    kind: TokenKind,
    func: &str,
    operand: fn(&mut Input<'a>) -> PResult<IdedExpr>,
) -> PResult<IdedExpr> {
    let first = operand(i)?;
    let Some(mut op) = eat(i, kind) else {
        return Ok(first);
    };
    let mut terms = vec![first];
    let mut ops = Vec::new();
    loop {
        let next = operand(i)?;
        ops.push(i.state.next_id(&op));
        terms.push(next);
        match eat(i, kind) {
            Some(next_op) => op = next_op,
            None => break,
        }
    }
    Ok(balanced_tree(func, &mut terms, &ops, 0, ops.len() - 1))
}

fn balanced_tree(
    func: &str,
    terms: &mut [IdedExpr],
    ops: &[u64],
    lo: usize,
    hi: usize,
) -> IdedExpr {
    let mid = (lo + hi).div_ceil(2);
    let left = if mid == lo {
        mem::take(&mut terms[mid])
    } else {
        balanced_tree(func, terms, ops, lo, mid - 1)
    };
    let right = if mid == hi {
        mem::take(&mut terms[mid + 1])
    } else {
        balanced_tree(func, terms, ops, mid + 1, hi)
    };
    call(ops[mid], func, vec![left, right])
}

const RELATION_OPS: [(TokenKind, &str); 7] = [
    (TokenKind::Less, operators::LESS),
    (TokenKind::LessEq, operators::LESS_EQUALS),
    (TokenKind::GreaterEq, operators::GREATER_EQUALS),
    (TokenKind::Greater, operators::GREATER),
    (TokenKind::EqEq, operators::EQUALS),
    (TokenKind::NotEq, operators::NOT_EQUALS),
    (TokenKind::In, operators::IN),
];

const ADDITIVE_OPS: [(TokenKind, &str); 2] = [
    (TokenKind::Plus, operators::ADD),
    (TokenKind::Minus, operators::SUBSTRACT),
];

const MULTIPLICATIVE_OPS: [(TokenKind, &str); 3] = [
    (TokenKind::Star, operators::MULTIPLY),
    (TokenKind::Slash, operators::DIVIDE),
    (TokenKind::Percent, operators::MODULO),
];

/// `relation : calc | relation ('<'|'<='|'>='|'>'|'=='|'!='|'in') relation`
fn relation<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    binary_chain(i, &RELATION_OPS, addition)
}

/// The `calc ('+'|'-') calc` alternatives of `calc`.
fn addition<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    binary_chain(i, &ADDITIVE_OPS, multiplication)
}

/// The `calc ('*'|'/'|'%') calc` alternatives of `calc`.
fn multiplication<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    binary_chain(i, &MULTIPLICATIVE_OPS, unary)
}

/// Left-associative binary operators of one precedence level.
fn binary_chain<'a>(
    i: &mut Input<'a>,
    ops: &[(TokenKind, &'static str)],
    operand: fn(&mut Input<'a>) -> PResult<IdedExpr>,
) -> PResult<IdedExpr> {
    let mut lhs = operand(i)?;
    while let Some(op) = i.first().copied() {
        let Some((_, func)) = ops.iter().find(|(kind, _)| *kind == op.kind) else {
            break;
        };
        i.next_token();
        let op_id = i.state.next_id(&op);
        let rhs = operand(i)?;
        lhs = call(op_id, func, vec![lhs, rhs]);
    }
    Ok(lhs)
}

/// `unary : member | '!'+ member | '-'+ member`
fn unary<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    if let Some(first) = eat(i, TokenKind::Exclam) {
        let mut count = 1;
        while eat(i, TokenKind::Exclam).is_some() {
            count += 1;
        }
        return prefixed(i, first, count, operators::LOGICAL_NOT);
    }
    if peek_kind(i) == Some(TokenKind::Minus) {
        let mut count = 1;
        while kind_at(i, count) == Some(TokenKind::Minus) {
            count += 1;
        }
        // A lone `-` right before a numeric literal is the literal's sign:
        // the `literal` alternative wins over `Negate` in the grammar.
        if count == 1 && matches!(kind_at(i, 1), Some(TokenKind::Int | TokenKind::Float)) {
            return member(i);
        }
        let first = expect(i, TokenKind::Minus)?;
        for _ in 1..count {
            i.next_token();
        }
        return prefixed(i, first, count, operators::NEGATE);
    }
    member(i)
}

/// A run of `count` identical prefix operators. Even runs cancel out and
/// get no id; an odd run is a single call, numbered from its first operator
/// before the operand is parsed.
fn prefixed<'a>(i: &mut Input<'a>, first: Token, count: usize, func: &str) -> PResult<IdedExpr> {
    if count % 2 == 0 {
        return member(i);
    }
    let op_id = i.state.next_id(&first);
    let target = member(i)?;
    Ok(call(op_id, func, vec![target]))
}

/// ```text
/// member : primary
///        | member '.' '?'? escapeIdent
///        | member '.' IDENTIFIER '(' exprList? ')'
///        | member '[' '?'? expr ']'
/// ```
fn member<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let mut lhs = primary(i)?;
    loop {
        if let Some(dot) = eat(i, TokenKind::Dot) {
            let optional = eat(i, TokenKind::Question);
            match peek_kind(i) {
                Some(TokenKind::Ident) => {
                    let ident = expect(i, TokenKind::Ident)?;
                    if optional.is_none() && peek_kind(i) == Some(TokenKind::LParen) {
                        let open = expect(i, TokenKind::LParen)?;
                        let op_id = i.state.next_id(&open);
                        let args = arguments(i)?;
                        let func_name = i.state.text(&ident).to_string();
                        lhs = call_or_macro(i, op_id, func_name, Some(lhs), args);
                    } else {
                        let field = i.state.text(&ident).to_string();
                        lhs = select(i, lhs, dot, optional, field);
                    }
                }
                Some(TokenKind::EscIdent) => {
                    let ident = expect(i, TokenKind::EscIdent)?;
                    let field = escaped_ident(i, &ident, &dot);
                    lhs = select(i, lhs, dot, optional, field);
                }
                _ => return Err(ErrMode::Cut(syntax_err(i, "identifier"))),
            }
        } else if let Some(open) = eat(i, TokenKind::LBracket) {
            let op_id = i.state.next_id(&open);
            let optional = eat(i, TokenKind::Question);
            let index = expr(i)?;
            expect_close(i, TokenKind::RBracket)?;
            let func = match optional {
                Some(_) if i.state.options.enable_optional_syntax => operators::OPT_INDEX,
                Some(_) => {
                    i.state.report_at(&open, "unsupported syntax '[?'");
                    operators::INDEX
                }
                None => operators::INDEX,
            };
            lhs = call(op_id, func, vec![lhs, index]);
        } else {
            return Ok(lhs);
        }
    }
}

fn select(
    i: &mut Input<'_>,
    operand: IdedExpr,
    dot: Token,
    optional: Option<Token>,
    field: String,
) -> IdedExpr {
    match optional {
        Some(_) if i.state.options.enable_optional_syntax => {
            let field_literal = i
                .state
                .next_expr(&dot, Expr::Literal(LiteralValue::String(field.into())));
            let op_id = i.state.next_id(&dot);
            call(op_id, operators::OPT_SELECT, vec![operand, field_literal])
        }
        Some(_) => {
            i.state.report_at(&dot, "unsupported syntax '.?'");
            plain_select(i, operand, dot, field)
        }
        None => plain_select(i, operand, dot, field),
    }
}

fn plain_select(i: &mut Input<'_>, operand: IdedExpr, dot: Token, field: String) -> IdedExpr {
    i.state.next_expr(
        &dot,
        Expr::Select(SelectExpr {
            operand: Box::new(operand),
            field,
            test: false,
        }),
    )
}

/// Strips the backticks off an `ESC_IDENTIFIER`, reporting at `at` when the
/// escape syntax isn't enabled.
fn escaped_ident(i: &mut Input<'_>, ident: &Token, at: &Token) -> String {
    if !i.state.options.enable_ident_escape_syntax {
        i.state.report_at(at, "unsupported syntax: '`'");
    }
    let raw = i.state.text(ident);
    raw[1..raw.len() - 1].to_string()
}

/// `'(' exprList? ')'` with the `(` already consumed —
/// `exprList : expr (',' expr)*`, no trailing comma.
fn arguments<'a>(i: &mut Input<'a>) -> PResult<Vec<IdedExpr>> {
    let mut args = Vec::new();
    if !matches!(peek_kind(i), Some(TokenKind::RParen) | None) {
        loop {
            args.push(recover(i, expr)?);
            if eat(i, TokenKind::Comma).is_none() {
                break;
            }
            if peek_kind(i) == Some(TokenKind::RParen) {
                recover_here(i, "expression")?;
                break;
            }
        }
    }
    expect_close(i, TokenKind::RParen)?;
    Ok(args)
}

/// ```text
/// primary : '.'? IDENTIFIER
///         | '.'? IDENTIFIER '(' exprList? ')'
///         | '(' expr ')'
///         | '[' listInit? ','? ']'
///         | '{' mapInitializerList? ','? '}'
///         | '.'? IDENTIFIER ('.' IDENTIFIER)* '{' fieldInitializerList? ','? '}'
///         | literal
/// ```
fn primary<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let Some(tok) = i.first().copied() else {
        return Err(ErrMode::Cut(syntax_err(i, "expression")));
    };
    match tok.kind {
        TokenKind::LParen => {
            i.next_token();
            let nested = expr(i)?;
            expect_close(i, TokenKind::RParen)?;
            Ok(nested)
        }
        TokenKind::LBracket => list(i),
        TokenKind::LBrace => map(i),
        TokenKind::Dot | TokenKind::Ident => ident_call_or_message(i),
        TokenKind::Minus => {
            // `literal : '-'? NUM_INT | '-'? NUM_FLOAT | …`
            i.next_token();
            match peek_kind(i) {
                Some(TokenKind::Int) => int_literal(i, true),
                Some(TokenKind::Float) => double_literal(i, true),
                _ => Err(ErrMode::Cut(syntax_err(i, "numeric literal"))),
            }
        }
        TokenKind::Int => int_literal(i, false),
        TokenKind::Uint => uint_literal(i),
        TokenKind::Float => double_literal(i, false),
        TokenKind::String => string_literal(i),
        TokenKind::Bytes => bytes_literal(i),
        TokenKind::True => {
            i.next_token();
            Ok(i.state
                .next_expr(&tok, Expr::Literal(LiteralValue::Boolean(true.into()))))
        }
        TokenKind::False => {
            i.next_token();
            Ok(i.state
                .next_expr(&tok, Expr::Literal(LiteralValue::Boolean(false.into()))))
        }
        TokenKind::Null => {
            i.next_token();
            Ok(i.state.next_expr(&tok, Expr::Literal(LiteralValue::Null)))
        }
        TokenKind::Error => {
            // Already reported by the lexer; stand in for the expression.
            i.next_token();
            Ok(IdedExpr::default())
        }
        _ => Err(ErrMode::Cut(syntax_err(i, "expression"))),
    }
}

fn int_literal<'a>(i: &mut Input<'a>, negative: bool) -> PResult<IdedExpr> {
    let tok = expect(i, TokenKind::Int)?;
    let text = i.state.text(&tok);
    let (radix, digits) = match text.strip_prefix("0x") {
        Some(hex) => (16, hex),
        None => (10, text),
    };
    let value = if negative {
        i64::from_str_radix(&format!("-{digits}"), radix)
    } else {
        i64::from_str_radix(digits, radix)
    };
    Ok(match value {
        Ok(v) => i
            .state
            .next_expr(&tok, Expr::Literal(LiteralValue::Int(v.into()))),
        Err(_) => {
            i.state.report_at(&tok, "invalid int literal");
            IdedExpr::default()
        }
    })
}

fn uint_literal<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let tok = expect(i, TokenKind::Uint)?;
    let text = i.state.text(&tok);
    let digits = &text[..text.len() - 1]; // strip the `u` / `U`
    let value = match digits.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => digits.parse::<u64>(),
    };
    Ok(match value {
        Ok(v) => i
            .state
            .next_expr(&tok, Expr::Literal(LiteralValue::UInt(v.into()))),
        Err(_) => {
            i.state.report_at(&tok, "invalid uint literal");
            IdedExpr::default()
        }
    })
}

fn double_literal<'a>(i: &mut Input<'a>, negative: bool) -> PResult<IdedExpr> {
    let tok = expect(i, TokenKind::Float)?;
    Ok(match i.state.text(&tok).parse::<f64>() {
        Ok(v) if v.is_finite() => {
            let v = if negative { -v } else { v };
            i.state
                .next_expr(&tok, Expr::Literal(LiteralValue::Double(v.into())))
        }
        _ => {
            i.state.report_at(&tok, "invalid double literal");
            IdedExpr::default()
        }
    })
}

fn string_literal<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let tok = expect(i, TokenKind::String)?;
    Ok(match parse::parse_string(i.state.text(&tok)) {
        Ok(s) => i
            .state
            .next_expr(&tok, Expr::Literal(LiteralValue::String(s.into()))),
        Err(e) => {
            i.state
                .report_at(&tok, format!("invalid string literal: {e:?}"));
            IdedExpr::default()
        }
    })
}

fn bytes_literal<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let tok = expect(i, TokenKind::Bytes)?;
    Ok(match parse::parse_bytes(i.state.text(&tok)) {
        Ok(bytes) => i
            .state
            .next_expr(&tok, Expr::Literal(LiteralValue::Bytes(bytes.into()))),
        Err(e) => {
            i.state
                .report_at(&tok, format!("invalid bytes literal: {e:?}"));
            IdedExpr::default()
        }
    })
}

/// `'[' listInit? ','? ']'` — `listInit : optExpr (',' optExpr)*`,
/// `optExpr : '?'? expr`. Note `[,]` is a (valid) empty list.
fn list<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let open = expect(i, TokenKind::LBracket)?;
    let list_id = i.state.next_id(&open);
    let mut elements = Vec::new();
    let mut optional_indices = Vec::new();
    if matches!(
        peek_kind(i),
        Some(TokenKind::RBracket | TokenKind::Comma) | None
    ) {
        let _ = eat(i, TokenKind::Comma);
    } else {
        loop {
            let optional = eat(i, TokenKind::Question);
            let element = recover(i, expr)?;
            match optional {
                Some(_) if i.state.options.enable_optional_syntax => {
                    optional_indices.push(elements.len());
                }
                Some(question) => i.state.report_at(&question, "unsupported syntax '?'"),
                None => {}
            }
            elements.push(element);
            if eat(i, TokenKind::Comma).is_none() || peek_kind(i) == Some(TokenKind::RBracket) {
                break;
            }
        }
    }
    expect_close(i, TokenKind::RBracket)?;
    Ok(IdedExpr {
        id: list_id,
        expr: Expr::List(ListExpr::new_with_optionals(elements, optional_indices)),
    })
}

/// `'{' mapInitializerList? ','? '}'` —
/// `mapInitializerList : optExpr ':' expr (',' optExpr ':' expr)*`
fn map<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let open = expect(i, TokenKind::LBrace)?;
    let map_id = i.state.next_id(&open);
    let entries = entries(i, TokenKind::RBrace, map_entry)?;
    expect_close(i, TokenKind::RBrace)?;
    Ok(IdedExpr {
        id: map_id,
        expr: Expr::Map(MapExpr { entries }),
    })
}

/// The comma-separated entry list of map and message literals, up to (not
/// including) `close`. A trailing comma is allowed, and `{,}` is empty.
fn entries<'a>(
    i: &mut Input<'a>,
    close: TokenKind,
    entry: fn(&mut Input<'a>) -> PResult<IdedEntryExpr>,
) -> PResult<Vec<IdedEntryExpr>> {
    let mut entries = Vec::new();
    match peek_kind(i) {
        Some(k) if k == close => return Ok(entries),
        Some(TokenKind::Comma) => {
            let _ = eat(i, TokenKind::Comma);
            return Ok(entries);
        }
        None => return Ok(entries),
        Some(_) => {}
    }
    loop {
        if let Some(e) = recover(i, |i| entry(i).map(Some))? {
            entries.push(e);
        }
        if eat(i, TokenKind::Comma).is_none() || peek_kind(i) == Some(close) {
            break;
        }
    }
    Ok(entries)
}

fn map_entry<'a>(i: &mut Input<'a>) -> PResult<IdedEntryExpr> {
    let optional = eat(i, TokenKind::Question);
    // The ANTLR visitor numbers an entry (from its `:`) before visiting the
    // key: reserve the id now, attach the span once the colon is reached.
    let entry_id = i.state.reserve_id();
    let key = expr(i)?;
    let colon = expect(i, TokenKind::Colon)?;
    i.state.set_offset(entry_id, &colon);
    let optional = match optional {
        Some(_) if i.state.options.enable_optional_syntax => true,
        Some(question) => {
            i.state.report_at(&question, "unsupported syntax '?'");
            false
        }
        None => false,
    };
    let value = expr(i)?;
    Ok(IdedEntryExpr {
        id: entry_id,
        expr: EntryExpr::MapEntry(MapEntryExpr {
            key,
            value,
            optional,
        }),
    })
}

/// The `'.'? IDENTIFIER …` alternatives of `primary`: an identifier, a
/// global call, or a message literal.
fn ident_call_or_message<'a>(i: &mut Input<'a>) -> PResult<IdedExpr> {
    let leading_dot = eat(i, TokenKind::Dot).is_some();
    if peek_kind(i) != Some(TokenKind::Ident) {
        return Err(ErrMode::Cut(syntax_err(i, "identifier")));
    }
    if message_ahead(i) {
        return message(i, leading_dot);
    }
    let ident = expect(i, TokenKind::Ident)?;
    let raw = i.state.text(&ident);
    let reserved = is_reserved_id(raw);
    let name = if leading_dot {
        format!(".{raw}")
    } else {
        raw.to_string()
    };
    if let Some(open) = eat(i, TokenKind::LParen) {
        if reserved {
            i.state
                .report_at(&ident, format!("reserved identifier: {raw}"));
        }
        let op_id = i.state.next_id(&open);
        let args = arguments(i)?;
        Ok(call_or_macro(i, op_id, name, None, args))
    } else {
        if reserved {
            i.state
                .report_at(&ident, format!("reserved identifier: {name}"));
        }
        Ok(i.state.next_expr(&ident, Expr::Ident(name)))
    }
}

/// Looks past `IDENTIFIER ('.' IDENTIFIER)*` for the `{` that makes this a
/// message literal (the lookahead ANTLR's prediction performs).
fn message_ahead(i: &Input<'_>) -> bool {
    let mut n = 1;
    while kind_at(i, n) == Some(TokenKind::Dot) && kind_at(i, n + 1) == Some(TokenKind::Ident) {
        n += 2;
    }
    kind_at(i, n) == Some(TokenKind::LBrace)
}

/// `'.'? IDENTIFIER ('.' IDENTIFIER)* '{' fieldInitializerList? ','? '}'` —
/// `fieldInitializerList : optField ':' expr (',' optField ':' expr)*`
fn message<'a>(i: &mut Input<'a>, leading_dot: bool) -> PResult<IdedExpr> {
    let mut type_name = String::new();
    if leading_dot {
        type_name.push('.');
    }
    loop {
        let ident = expect(i, TokenKind::Ident)?;
        type_name.push_str(i.state.text(&ident));
        if eat(i, TokenKind::Dot).is_none() {
            break;
        }
        type_name.push('.');
    }
    let open = expect(i, TokenKind::LBrace)?;
    let struct_id = i.state.next_id(&open);
    let entries = entries(i, TokenKind::RBrace, message_field)?;
    expect_close(i, TokenKind::RBrace)?;
    Ok(IdedExpr {
        id: struct_id,
        expr: Expr::Struct(StructExpr { type_name, entries }),
    })
}

/// `optField ':' expr` — `optField : '?'? escapeIdent`
fn message_field<'a>(i: &mut Input<'a>) -> PResult<IdedEntryExpr> {
    let optional = eat(i, TokenKind::Question);
    let name = match peek_kind(i) {
        Some(kind @ (TokenKind::Ident | TokenKind::EscIdent)) => expect(i, kind)?,
        _ => return Err(ErrMode::Cut(syntax_err(i, "identifier"))),
    };
    let colon = expect(i, TokenKind::Colon)?;
    let entry_id = i.state.next_id(&colon);
    let field = if name.kind == TokenKind::EscIdent {
        escaped_ident(i, &name, optional.as_ref().unwrap_or(&name))
    } else {
        i.state.text(&name).to_string()
    };
    let optional = match optional {
        Some(_) if i.state.options.enable_optional_syntax => true,
        Some(question) => {
            i.state.report_at(&question, "unsupported syntax '?'");
            false
        }
        None => false,
    };
    let value = expr(i)?;
    Ok(IdedEntryExpr {
        id: entry_id,
        expr: EntryExpr::StructField(StructFieldExpr {
            field,
            value,
            optional,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::ast::{ComprehensionExpr, EntryExpr, Expr, LiteralValue};
    use crate::IdedExpr;
    use std::iter;

    type Parser = WinnowParser;

    #[derive(Default)]
    struct TestInfo {
        // I contains the input expression to be parsed.
        i: &'static str,

        // P contains the type/id adorned debug output of the expression tree.
        p: &'static str,

        // E contains the expected error output for a failed parse, or "" if the parse is expected to be successful.
        e: &'static str,

        // Options to be configured with the parser before parsing the expression.
        enable_optional_syntax: bool,
    }

    // -----------------------------------------------------------------
    // Lexer
    // -----------------------------------------------------------------

    fn kinds(source: &str) -> Vec<TokenKind> {
        let (tokens, errors) = lex(source);
        assert!(errors.is_empty(), "unexpected lexer errors: {errors:?}");
        tokens.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_every_token_kind() {
        use TokenKind::*;
        assert_eq!(
            kinds("a.b[0]{}(),?:-+*/%! == != < <= > >= && || in true false null 1 1u 1.5 'x' b'y' `z`"),
            vec![
                Ident, Dot, Ident, LBracket, Int, RBracket, LBrace, RBrace, LParen, RParen, Comma,
                Question, Colon, Minus, Plus, Star, Slash, Percent, Exclam, EqEq, NotEq, Less,
                LessEq, Greater, GreaterEq, And, Or, In, True, False, Null, Int, Uint, Float,
                String, Bytes, EscIdent,
            ]
        );
    }

    #[test]
    fn lexes_spans_and_skips_trivia() {
        let (tokens, errors) = lex(" a // comment\n\t.b ");
        assert!(errors.is_empty());
        let spans: Vec<_> = tokens.iter().map(|t| (t.kind, t.start, t.end)).collect();
        assert_eq!(
            spans,
            vec![
                (TokenKind::Ident, 1, 2),
                (TokenKind::Dot, 15, 16),
                (TokenKind::Ident, 16, 17),
            ]
        );
    }

    #[test]
    fn lexes_numbers_like_antlr() {
        use TokenKind::*;
        // `1.` is an int followed by a dot: a float needs digits after the dot.
        assert_eq!(kinds("1."), vec![Int, Dot]);
        assert_eq!(kinds("1.5"), vec![Float]);
        assert_eq!(kinds(".5e-3"), vec![Float]);
        assert_eq!(kinds("1e5"), vec![Float]);
        assert_eq!(kinds("1E+2"), vec![Float]);
        // An exponent needs digits too.
        assert_eq!(kinds("1e"), vec![Int, Ident]);
        // Only a lowercase `x` introduces a hex literal, and it needs digits.
        assert_eq!(kinds("0x1F"), vec![Int]);
        assert_eq!(kinds("0x1Fu"), vec![Uint]);
        assert_eq!(kinds("0x"), vec![Int, Ident]);
        assert_eq!(kinds("0X1"), vec![Int, Ident]);
        // The uint suffix only applies to integers.
        assert_eq!(kinds("1u"), vec![Uint]);
        assert_eq!(kinds("1U"), vec![Uint]);
        assert_eq!(kinds("1.5u"), vec![Float, Ident]);
        assert_eq!(kinds("1e5u"), vec![Float, Ident]);
    }

    #[test]
    fn lexes_strings_and_bytes() {
        use TokenKind::*;
        assert_eq!(kinds(r#""a\"b" 'c\'d'"#), vec![String, String]);
        assert_eq!(kinds(r#""""a"b""" '''c'd'''"#), vec![String, String]);
        assert_eq!(kinds(r#"r"a\" R'b\'"#), vec![String, String]);
        assert_eq!(kinds("r\"\"\"a\nb\"\"\""), vec![String]);
        assert_eq!(
            kinds(r#"b'a' B"b" br'c' bR"d" Br'e' BR"f""#),
            vec![Bytes; 6]
        );
        // `rb` is not a bytes prefix in the grammar: it lexes as an identifier.
        assert_eq!(kinds(r#"rb'x'"#), vec![Ident, String]);
        // Prefix letters not followed by a quote are just identifiers.
        assert_eq!(kinds("r b br rb"), vec![Ident; 4]);
    }

    #[test]
    fn lexer_errors_produce_error_tokens() {
        fn lex_errors(source: &str) -> (Vec<TokenKind>, Vec<LexError>) {
            let (tokens, errors) = lex(source);
            (tokens.into_iter().map(|t| t.kind).collect(), errors)
        }
        use TokenKind::*;

        let (kinds, errors) = lex_errors("a @ b");
        assert_eq!(kinds, vec![Ident, Error, Ident]);
        assert_eq!(
            errors,
            vec![LexError {
                offset: 2,
                message: "unexpected character"
            }]
        );

        let (kinds, errors) = lex_errors("a | b");
        assert_eq!(kinds, vec![Ident, Error, Ident]);
        assert_eq!(errors[0].message, "unexpected single '|', expected '||'");

        let (kinds, errors) = lex_errors("a & b");
        assert_eq!(kinds, vec![Ident, Error, Ident]);
        assert_eq!(errors[0].message, "unexpected single '&', expected '&&'");

        // An unterminated literal swallows the rest of the input.
        let (kinds, errors) = lex_errors("'abc + 1");
        assert_eq!(kinds, vec![Error]);
        assert_eq!(errors[0].message, "unterminated string literal");
        let (kinds, errors) = lex_errors("b'abc");
        assert_eq!(kinds, vec![Error]);
        assert_eq!(errors[0].message, "unterminated bytes literal");
        // A single-quoted literal can't span lines.
        let (_, errors) = lex_errors("'a\nb'");
        assert_eq!(errors[0].message, "unterminated string literal");

        let (_, errors) = lex_errors("`a");
        assert_eq!(errors[0].message, "unterminated quoted identifier");
        let (kinds, errors) = lex_errors("`a$b`.c");
        assert_eq!(kinds, vec![Error, Dot, Ident]);
        assert_eq!(errors[0].message, "invalid quoted identifier");
        let (_, errors) = lex_errors("``");
        assert_eq!(errors[0].message, "invalid quoted identifier");

        // Non-ASCII garbage is skipped one character at a time, not one byte.
        let (kinds, errors) = lex_errors("a ✌ b");
        assert_eq!(kinds, vec![Ident, Error, Ident]);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn positions_are_one_based_lines_and_columns() {
        assert_eq!(pos_for_offset("ab\ncd", 0), (1, 1));
        assert_eq!(pos_for_offset("ab\ncd", 2), (1, 3));
        assert_eq!(pos_for_offset("ab\ncd", 3), (2, 1));
        assert_eq!(pos_for_offset("ab\ncd", 5), (2, 3));
        assert_eq!(pos_for_offset("ab\n", 3), (2, 1));
        assert_eq!(pos_for_offset("", 0), (1, 1));
    }

    // -----------------------------------------------------------------
    // Equivalence with the ANTLR parser
    // -----------------------------------------------------------------

    /// Every id in the tree, so their source offsets can be compared too.
    fn ids(expr: &IdedExpr, out: &mut Vec<u64>) {
        out.push(expr.id);
        match &expr.expr {
            Expr::Unspecified | Expr::Ident(_) | Expr::Literal(_) => {}
            Expr::Call(call) => {
                if let Some(target) = &call.target {
                    ids(target, out);
                }
                call.args.iter().for_each(|arg| ids(arg, out));
            }
            Expr::Comprehension(c) => {
                for e in [
                    &c.iter_range,
                    &c.accu_init,
                    &c.loop_cond,
                    &c.loop_step,
                    &c.result,
                ] {
                    ids(e, out);
                }
            }
            Expr::List(list) => list.elements.iter().for_each(|e| ids(e, out)),
            Expr::Select(select) => ids(&select.operand, out),
            Expr::Map(MapExpr { entries }) | Expr::Struct(StructExpr { entries, .. }) => {
                for entry in entries {
                    out.push(entry.id);
                    match &entry.expr {
                        EntryExpr::StructField(field) => ids(&field.value, out),
                        EntryExpr::MapEntry(entry) => {
                            ids(&entry.key, out);
                            ids(&entry.value, out);
                        }
                    }
                }
            }
        }
    }

    fn assert_same_as_antlr(source: &str, optional: bool, escapes: bool) {
        let antlr = crate::parser::Parser::new()
            .enable_optional_syntax(optional)
            .enable_ident_escape_syntax(escapes)
            .parse_with_source_info(source);
        let winnow = Parser::new()
            .enable_optional_syntax(optional)
            .enable_ident_escape_syntax(escapes)
            .parse_with_source_info(source);
        match (antlr, winnow) {
            (Ok((expected, expected_info)), Ok((actual, actual_info))) => {
                assert_eq!(actual, expected, "AST for `{source}`");
                let mut all = Vec::new();
                ids(&expected, &mut all);
                for id in all {
                    assert_eq!(
                        actual_info.offset_for(id),
                        expected_info.offset_for(id),
                        "offsets of #{id} for `{source}`"
                    );
                }
            }
            (Ok(_), Err(e)) => panic!("`{source}`: ANTLR parses it, winnow doesn't: {e}"),
            (Err(e), Ok(_)) => panic!("`{source}`: winnow parses it, ANTLR doesn't: {e}"),
            (Err(_), Err(_)) => panic!("`{source}`: neither parser accepts it"),
        }
    }

    #[test]
    fn produces_the_same_ast_as_the_antlr_parser() {
        let sources = [
            // Identifiers, selection, calls, indexing
            "a",
            ".a",
            "a.b.c",
            "a . b",
            "a()",
            "a(b, c)",
            ".a.b(c)",
            "a.b(c).d",
            "a .b ( )",
            "a[b][c]",
            "a[0].b",
            "a.b().c()",
            "f(g(h(1)))",
            "a.while",
            "a.while()",
            "a.true1",
            "size(x) > 0 && x[0] == 'a'",
            // Literals
            "1",
            "-1",
            "-9223372036854775808",
            "0x1F",
            "-0x1F",
            "0x1Fu",
            "1u",
            "1U",
            "1.5",
            "-1.5",
            ".5",
            "1e5",
            "1.5e-3",
            "1E+2",
            "true",
            "false",
            "null",
            "\"a\"",
            "'a'",
            "\"\"\"a\"b\"\"\"",
            "'''a'b'''",
            "r\"a\\nb\"",
            "R'a\\nb'",
            "\"\\a\\b\\f\\n\\r\\t\\v'\\\"\\\\ \\u2764 \\U0001f431 \\x41 \\101\"",
            "'✌'",
            "b'abc'",
            "B\"abc\"",
            "br'a\\b'",
            "b'\\xFF\\376'",
            // Unary
            "!a",
            "!!a",
            "!!!a",
            "!-1",
            "!!-1",
            "-a",
            "--a",
            "---a",
            "--1",
            "---1",
            "-(a)",
            "-1u",
            "-a.b",
            "-f(1)",
            "4--4",
            "4--4.1",
            "4 - -4",
            "4 - - 4",
            // Binary
            "1 + 2 * 3 - 4 / 5 % 6",
            "a * b + c",
            "a + b * c",
            "a - b - c",
            "a < b < c",
            "a == b != c",
            "a in b",
            "a in b in c",
            "a && b || c && d",
            "a || b || c || d || e || f",
            "a && b && c && d && e && f && g",
            "a && b && c && d || e && f && g && h",
            // Ternary
            "a ? b : c",
            "a ? b : c ? d : e",
            "(a ? b : c) ? d : e",
            "a || b ? c && d : e",
            "a ? b + 1 : c * 2",
            // Lists
            "[]",
            "[,]",
            "[1,]",
            "[1, 2, 3]",
            "[[1], [2, [3]]]",
            "[] + [1, 2, 3] + [4]",
            // Maps
            "{}",
            "{,}",
            "{1: 2,}",
            "{a: b, c: d}",
            "{'a': 1, 'b': [2]}",
            "{a ? b : c : d}",
            // Messages
            "foo{}",
            "foo{,}",
            "foo{a: 1,}",
            "foo{ a:b, c:d }",
            "a.b.c{d: e}",
            ".a.b{c: d}",
            "Foo{a: Bar{b: 1}}",
            "SomeMessage{foo: 5, bar: \"xyz\"}",
            // Macros
            "has(m.f)",
            "has(a.b.c)",
            "m.exists(v, f)",
            "m.all(v, f)",
            "m.exists_one(v, f)",
            "m.existsOne(v, f)",
            "m.map(v, f)",
            "m.map(v, p, f)",
            "m.filter(v, p)",
            "x.filter(y, y.exists(z, has(z.a)))",
            "[1, 2, 3].all(x, x > 0)",
            "{}.map(a, a.map(b, b.map(c, c)))",
            "has(a.b) && a.b.exists(c, c > 1)",
            // Trivia
            "  a  ",
            "a // comment\n.b",
            "a\n+\nb",
            "\n// leading comment\nthis.is.not()\n\n// trailing\n\n",
        ];
        for source in sources {
            assert_same_as_antlr(source, false, false);
        }

        let optional_sources = [
            "a.?b",
            "a.?b.c",
            "a.?b[?0] && a[?c]",
            "[?a, ?b]",
            "[?a[?b]]",
            "[1, ?a]",
            "{?'key': value}",
            "{?a: b, c: d}",
            "Msg{?field: value}",
            "Msg{a: 1, ?b: 2}",
            "a.`b-c`",
            "{'a/b': 1}.`a/b`",
            "Msg{`f.g`: 1, ?`h i`: 2}",
            "a.?`b c`",
        ];
        for source in optional_sources {
            assert_same_as_antlr(source, true, true);
        }
    }

    #[test]
    fn rejects_what_the_antlr_parser_rejects() {
        let sources = [
            "",
            "\n",
            "1 + ()",
            "/",
            ".",
            "@foo",
            "x(1,)",
            "!-\u{1}",
            "1 + $",
            "break",
            "namespace(1)",
            "\"\\xFh\"",
            "f(*, *, *)",
            "!-a",
            "-!a",
            "a.?b()",
            "a.`b`()",
            "a ? b ? c : d : e",
            "[1 2]",
            "{a}",
            "{a:}",
            "Foo{a}",
            "Foo{1: 2}",
            "a.5",
            "1.",
            "0x",
            "0X1",
            "1e",
            "'abc",
            "\"a\nb\"",
            "rb'x'",
            "a b",
            "a.in",
            "in",
            "a = b",
            "&&",
            "a && || b",
            "a |",
            "`a`",
            "a.``",
            "1u.5",
            "\x0b",
            "a.b(c){}",
            "(a",
            "a[1",
            "[1,,]",
            "[?a]",
            "a.?b",
            "{?a: 1}",
            "Foo{?a: 1}",
            "a.`b`",
        ];
        for source in sources {
            assert!(
                crate::parser::Parser::new().parse(source).is_err(),
                "`{source}` should be rejected by the ANTLR parser"
            );
            assert!(
                Parser::new().parse(source).is_err(),
                "`{source}` should be rejected"
            );
        }
        let deep = "(".repeat(20) + "a";
        assert!(Parser::new().parse(&deep).is_err());
    }

    #[test]
    fn reports_errors_where_the_antlr_parser_does() {
        let sources = [
            "a b",
            "[1 2]",
            "(a",
            "{",
            "foo(a,b,)",
            "1 + ",
            "a.",
            "a[1",
            "has(m)",
            "1.all(2, 3)",
            "0xFFFFFFFFFFFFFFFFF",
            "break",
            "a.?b",
            "[?a]",
            "Msg{?field: value}",
            "a.`b`",
            "a\n+\n",
        ];
        for source in sources {
            let expected = crate::parser::Parser::new()
                .parse(source)
                .expect_err("ANTLR rejects it");
            let actual = Parser::new().parse(source).expect_err("winnow rejects it");
            assert_eq!(
                actual.errors[0].pos, expected.errors[0].pos,
                "position of the first error for `{source}`\n  antlr: {expected}\n  winnow: {actual}"
            );
        }
    }

    // -----------------------------------------------------------------
    // Ported from the ANTLR parser's tests
    // -----------------------------------------------------------------

    #[test]
    fn test_bad_input() {
        let expressions = [
            "1 + ()", "/", ".", "@foo", "x(1,)", "\x0a", "\n", "", "!-\u{1}",
        ];
        for expr in expressions {
            assert!(
                Parser::new().parse(expr).is_err(),
                "Expression `{}` should not parse",
                expr
            );
        }
    }

    #[test]
    fn test_comments() {
        let expression = r#"
        // This is a comment
        this.is.not()

        // We don't care!

        "#;
        assert!(Parser::new().parse(expression).is_ok());
    }

    #[test]
    fn recursion_limits() {
        let expressions = [
            "[[[1]]]",
            "(((1)))",
            "{1: {2: {3: 'none'}}}",
            "type(type(type(1)))",
            "[{'a': size([])}]",
            "{}.map(a, a.map(b, b.map(c, c)))",
        ];
        for expr in expressions {
            assert!(
                Parser::new().max_recursion_depth(3).parse(expr).is_ok(),
                "Expression `{}` should parse",
                expr
            );
            assert!(
                Parser::new().max_recursion_depth(2).parse(expr).is_err(),
                "Expression `{}` should not parse",
                expr
            );
        }
        let expressions = [
            "[[[[[[[[[[1]]]]]]]]]]",
            "((((((((((1))))))))))",
            "{1: {2: {3: {4: {5: {6: {1: {2: {3: {4: 'none'}}}}}}}}}}",
            "type(type(type(type(type(type(type(type(type(type(1))))))))))",
            "[{'a': size([{'1':size([{'1':size([[]])}])}])}]",
        ];
        for expr in expressions {
            assert!(
                Parser::new().max_recursion_depth(10).parse(expr).is_ok(),
                "Expression `{}` should parse",
                expr
            );
            assert!(
                Parser::new().max_recursion_depth(9).parse(expr).is_err(),
                "Expression `{}` should not parse",
                expr
            );
        }
        assert!(Parser::new().max_recursion_depth(0).parse("1 + 1").is_ok());
        assert!(Parser::new()
            .max_recursion_depth(0)
            .parse("(1 + 1)")
            .is_err());
    }

    #[test]
    fn recursion_limit_is_reported_once_and_aborts() {
        let err = Parser::new()
            .max_recursion_depth(2)
            .parse("[[[[1]]]] + [[[[2]]]]")
            .expect_err("too deep");
        assert_eq!(err.errors.len(), 1);
        assert_eq!(err.errors[0].msg, "expression recursion limit exceeded: 2");
    }

    #[test]
    fn recovery_limit_bails_out() {
        let expression = "[?, ?, ?, ?, ?]";
        let err = Parser::new()
            .error_recovery_limit(4)
            .parse(expression)
            .expect_err("expression should fail to parse");

        let rendered = format!("{err}");
        assert!(
            rendered.contains("error recovery attempt limit exceeded: 4"),
            "expected recovery limit error, got: {rendered}"
        );
    }

    #[test]
    fn recovery_limit_hit_by_pathological_nested_negation() {
        let expression = "!!(!!!!!!(!!!!(((((!!(!!(!!!!((!!(!!!!!!(!!!!(((((!!(!!(!!!!((1";
        let err = Parser::new()
            .error_recovery_limit(20)
            .parse(expression)
            .expect_err("expression should fail to parse");

        let rendered = format!("{err}");
        assert!(
            rendered.contains("error recovery attempt limit exceeded: 20"),
            "expected recovery limit error, got: {rendered}"
        );
    }

    #[test]
    fn recovery_limit_counts_lexer_errors() {
        let err = Parser::new()
            .error_recovery_limit(2)
            .parse("a @ b @ c @ d")
            .expect_err("should fail");
        let rendered = format!("{err}");
        assert!(
            rendered.contains("error recovery attempt limit exceeded: 2"),
            "expected recovery limit error, got: {rendered}"
        );
    }

    #[test]
    fn recovery_limit_permits_healthy_parses() {
        // Well-formed expressions never invoke recovery, so a tiny limit is fine.
        assert!(Parser::new()
            .error_recovery_limit(0)
            .parse("1 + 2 * 3")
            .is_ok());
    }

    #[test]
    fn recovers_and_reports_several_errors() {
        let err = Parser::new()
            .parse("[1 +, f(2 3), {a}, (4]")
            .expect_err("should fail");
        let positions: Vec<_> = err.errors.iter().map(|e| e.pos).collect();
        assert_eq!(positions, vec![(1, 5), (1, 11), (1, 17), (1, 22)], "{err}");
    }

    #[test]
    fn leading_dot_ident() {
        let expr = Parser::new()
            .parse(".x")
            .expect(".x should parse as a leading-dot ident");
        assert!(
            matches!(&expr.expr, crate::common::ast::Expr::Ident(s) if s == ".x"),
            "expected Ident(\".x\"), got {:?}",
            expr.expr
        );
    }

    #[test]
    fn reserved_identifiers_are_rejected() {
        // These are valid IDENTIFIER tokens in the lexer but must be rejected
        // by the parser (mirrors cel-go's reservedIds check).
        // `in`, `true`, `false`, `null` are grammar-level keywords rejected
        // earlier — they never make it to an identifier.
        for kw in &[
            "as",
            "break",
            "const",
            "continue",
            "else",
            "for",
            "function",
            "if",
            "import",
            "let",
            "loop",
            "package",
            "namespace",
            "return",
            "var",
            "void",
            "while",
        ] {
            let err = Parser::new().parse(kw).expect_err(&format!(
                "`{kw}` should be rejected as a reserved identifier"
            ));
            assert!(
                format!("{err}").contains("reserved identifier"),
                "expected reserved identifier error for `{kw}`, got: {err}"
            );
        }
        // Also rejected when used as a function name
        let err = Parser::new()
            .parse("namespace(1)")
            .expect_err("`namespace(1)` should fail");
        assert!(
            format!("{err}").contains("reserved identifier"),
            "expected reserved identifier error, got: {err}"
        );
    }

    #[test]
    fn backtick_field_selector_strips_backticks_when_enabled() {
        // With the flag on the parser should accept the source and lower it
        // to a plain `Select` whose field is the unescaped identifier.
        let expr = Parser::new()
            .enable_ident_escape_syntax(true)
            .parse("{'a/b': 1}.`a/b`")
            .expect("should parse with ident-escape enabled");
        // The outermost node is the Select — walk in and find its field.
        fn field_of(expr: &crate::common::ast::Expr) -> Option<&str> {
            match expr {
                crate::common::ast::Expr::Select(sel) => Some(&sel.field),
                _ => None,
            }
        }
        assert_eq!(field_of(&expr.expr), Some("a/b"));
    }

    #[test]
    fn backtick_field_selector_is_rejected_by_default() {
        // Per cel-spec (matching cel-go `parser/parser.go:161` with
        // `EnableIdentEscapeSyntax(false)`), the parser must reject
        // backtick-quoted field selectors unless the feature is opted in.
        let err = Parser::new()
            .parse("{'a/b': 1}.`a/b`")
            .expect_err("backtick selector must be rejected");
        assert!(
            format!("{err}").contains("unsupported syntax: '`'"),
            "expected `unsupported syntax` error, got: {err}"
        );
    }

    #[test]
    fn reserved_identifiers_are_valid_as_field_selectors() {
        // Per cel-spec, reserved words CAN be used as field-selector names in
        // Select expressions (`.as`, `.while`, etc.) — the reserved-id check
        // only applies to bare identifiers and function names.
        for kw in &[
            "as",
            "break",
            "const",
            "continue",
            "else",
            "for",
            "function",
            "if",
            "import",
            "let",
            "loop",
            "package",
            "namespace",
            "return",
            "var",
            "void",
            "while",
        ] {
            let expr = format!("{{ '{kw}': 1 }}.{kw}");
            Parser::new()
                .parse(&expr)
                .unwrap_or_else(|e| panic!("`{expr}` should parse but got: {e}"));
        }
    }

    // Even counts of `!` or `-` cancel out, and the operand is parsed once.
    #[test]
    fn even_unary_operators_visit_child_once() {
        // Even `!` → identity (no logical-not wrapper)
        let expr = Parser::new().parse("!!a").expect("!!a should parse");
        // !!a cancels to `a`; should be an Ident, not a Call
        assert!(
            matches!(expr.expr, crate::common::ast::Expr::Ident(_)),
            "!!a should reduce to an identity ident, got {:?}",
            expr.expr
        );

        // Even `-` → identity
        let expr = Parser::new().parse("--1").expect("--1 should parse");
        assert!(
            matches!(expr.expr, crate::common::ast::Expr::Literal(_)),
            "--1 should reduce to a literal, got {:?}",
            expr.expr
        );

        // Deeply nested even `--` must not cause exponential slowdown.
        let mut nested = "x".to_string();
        for _ in 0..30 {
            nested = format!("--({})", nested);
        }
        let result = Parser::new().parse(&nested);
        // May parse or error depending on depth limits, but must not hang.
        let _ = result;
    }

    #[test]
    fn malformed_nested_expression_does_not_panic() {
        let expression = "ma[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[\x0c\0\0\0\0\0\0\0[[[[[[[putTo?[[[[[[[[[[ep";

        assert!(Parser::new()
            .max_recursion_depth(48)
            .parse(expression)
            .is_err());
    }

    #[test]
    fn test() {
        let test_cases = [
            TestInfo {
                i: r#""A""#,
                p: r#""A"^#1:*expr.Constant_StringValue#"#,
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: r#"true"#,
                p: r#"true^#1:*expr.Constant_BoolValue#"#,
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: r#"false"#,
                p: r#"false^#1:*expr.Constant_BoolValue#"#,
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "0",
                p: "0^#1:*expr.Constant_Int64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "42",
                p: "42^#1:*expr.Constant_Int64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "0xF",
                p: "15^#1:*expr.Constant_Int64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "0u",
                p: "0u^#1:*expr.Constant_Uint64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "23u",
                p: "23u^#1:*expr.Constant_Uint64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "24u",
                p: "24u^#1:*expr.Constant_Uint64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "0xFu",
                p: "15u^#1:*expr.Constant_Uint64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "-1",
                p: "-1^#1:*expr.Constant_Int64Value#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "4--4",
                p: r#"_-_(
    4^#1:*expr.Constant_Int64Value#,
    -4^#3:*expr.Constant_Int64Value#
)^#2:*expr.Expr_CallExpr#"#,
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "4--4.1",
                p: r#"_-_(
    4^#1:*expr.Constant_Int64Value#,
    -4.1^#3:*expr.Constant_DoubleValue#
)^#2:*expr.Expr_CallExpr#"#,
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: r#"b"abc""#,
                p: r#"b"abc"^#1:*expr.Constant_BytesValue#"#,
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "23.39",
                p: "23.39^#1:*expr.Constant_DoubleValue#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "!a",
                p: "!_(
    a^#2:*expr.Expr_IdentExpr#
)^#1:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "null",
                p: "null^#1:*expr.Constant_NullValue#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a",
                p: "a^#1:*expr.Expr_IdentExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a?b:c",
                p: "_?_:_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#,
    c^#4:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a || b",
                p: "_||_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#2:*expr.Expr_IdentExpr#
)^#3:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a || b || c || d || e || f ",
                p: "_||_(
    _||_(
        _||_(
            a^#1:*expr.Expr_IdentExpr#,
            b^#2:*expr.Expr_IdentExpr#
        )^#3:*expr.Expr_CallExpr#,
        c^#4:*expr.Expr_IdentExpr#
    )^#5:*expr.Expr_CallExpr#,
    _||_(
        _||_(
            d^#6:*expr.Expr_IdentExpr#,
            e^#8:*expr.Expr_IdentExpr#
        )^#9:*expr.Expr_CallExpr#,
        f^#10:*expr.Expr_IdentExpr#
    )^#11:*expr.Expr_CallExpr#
)^#7:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a && b",
                p: "_&&_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#2:*expr.Expr_IdentExpr#
)^#3:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a && b && c && d && e && f && g",
                p: "_&&_(
    _&&_(
        _&&_(
            a^#1:*expr.Expr_IdentExpr#,
            b^#2:*expr.Expr_IdentExpr#
        )^#3:*expr.Expr_CallExpr#,
        _&&_(
            c^#4:*expr.Expr_IdentExpr#,
            d^#6:*expr.Expr_IdentExpr#
        )^#7:*expr.Expr_CallExpr#
    )^#5:*expr.Expr_CallExpr#,
    _&&_(
        _&&_(
            e^#8:*expr.Expr_IdentExpr#,
            f^#10:*expr.Expr_IdentExpr#
        )^#11:*expr.Expr_CallExpr#,
        g^#12:*expr.Expr_IdentExpr#
    )^#13:*expr.Expr_CallExpr#
)^#9:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a && b && c && d || e && f && g && h",
                p: "_||_(
    _&&_(
        _&&_(
            a^#1:*expr.Expr_IdentExpr#,
            b^#2:*expr.Expr_IdentExpr#
        )^#3:*expr.Expr_CallExpr#,
        _&&_(
            c^#4:*expr.Expr_IdentExpr#,
            d^#6:*expr.Expr_IdentExpr#
        )^#7:*expr.Expr_CallExpr#
    )^#5:*expr.Expr_CallExpr#,
    _&&_(
        _&&_(
            e^#8:*expr.Expr_IdentExpr#,
            f^#9:*expr.Expr_IdentExpr#
        )^#10:*expr.Expr_CallExpr#,
        _&&_(
            g^#11:*expr.Expr_IdentExpr#,
            h^#13:*expr.Expr_IdentExpr#
        )^#14:*expr.Expr_CallExpr#
    )^#12:*expr.Expr_CallExpr#
)^#15:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a + b",
                p: "_+_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a - b",
                p: "_-_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a * b",
                p: "_*_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a / b",
                p: "_/_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a % b",
                p: "_%_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a in b",
                p: "@in(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a == b",
                p: "_==_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a != b",
                p: "_!=_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a > b",
                p: "_>_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a >= b",
                p: "_>=_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a < b",
                p: "_<_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a <= b",
                p: "_<=_(
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a.b",
                p: "a^#1:*expr.Expr_IdentExpr#.b^#2:*expr.Expr_SelectExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a.b.c",
                p: "a^#1:*expr.Expr_IdentExpr#.b^#2:*expr.Expr_SelectExpr#.c^#3:*expr.Expr_SelectExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a[b]",
                p: "_[_](
    a^#1:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "(a)",
                p: "a^#1:*expr.Expr_IdentExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "((a))",
                p: "a^#1:*expr.Expr_IdentExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a()",
                p: "a()^#1:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a(b)",
                p: "a(
    b^#2:*expr.Expr_IdentExpr#
)^#1:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a(b, c)",
                p: "a(
    b^#2:*expr.Expr_IdentExpr#,
    c^#3:*expr.Expr_IdentExpr#
)^#1:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a.b()",
                p: "a^#1:*expr.Expr_IdentExpr#.b()^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "a.b(c)",
                p: "a^#1:*expr.Expr_IdentExpr#.b(
    c^#3:*expr.Expr_IdentExpr#
)^#2:*expr.Expr_CallExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "foo{ }",
                p: "foo{}^#1:*expr.Expr_StructExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "foo{ a:b }",
                p: "foo{
    a:b^#3:*expr.Expr_IdentExpr#^#2:*expr.Expr_CreateStruct_Entry#
}^#1:*expr.Expr_StructExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "foo{ a:b, c:d }",
                p: "foo{
    a:b^#3:*expr.Expr_IdentExpr#^#2:*expr.Expr_CreateStruct_Entry#,
    c:d^#5:*expr.Expr_IdentExpr#^#4:*expr.Expr_CreateStruct_Entry#
}^#1:*expr.Expr_StructExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "{}",
                p: "{}^#1:*expr.Expr_StructExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "{a: b, c: d}",
                p: "{
    a^#3:*expr.Expr_IdentExpr#:b^#4:*expr.Expr_IdentExpr#^#2:*expr.Expr_CreateStruct_Entry#,
    c^#6:*expr.Expr_IdentExpr#:d^#7:*expr.Expr_IdentExpr#^#5:*expr.Expr_CreateStruct_Entry#
}^#1:*expr.Expr_StructExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "[]",
                p: "[]^#1:*expr.Expr_ListExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "[a]",
                p: "[
    a^#2:*expr.Expr_IdentExpr#
]^#1:*expr.Expr_ListExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "[a, b, c]",
                p: "[
    a^#2:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#,
    c^#4:*expr.Expr_IdentExpr#
]^#1:*expr.Expr_ListExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "has(m.f)",
                p: "m^#2:*expr.Expr_IdentExpr#.f~test-only~^#4:*expr.Expr_SelectExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "m.exists(v, f)",
                p: "__comprehension__(
// Variable
v,
// Target
m^#1:*expr.Expr_IdentExpr#,
// Accumulator
@result,
// Init
false^#5:*expr.Constant_BoolValue#,
// LoopCondition
@not_strictly_false(
    !_(
        @result^#6:*expr.Expr_IdentExpr#
    )^#7:*expr.Expr_CallExpr#
)^#8:*expr.Expr_CallExpr#,
// LoopStep
_||_(
    @result^#9:*expr.Expr_IdentExpr#,
    f^#4:*expr.Expr_IdentExpr#
)^#10:*expr.Expr_CallExpr#,
// Result
@result^#11:*expr.Expr_IdentExpr#)^#12:*expr.Expr_ComprehensionExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "m.all(v, f)",
                p: "__comprehension__(
// Variable
v,
// Target
m^#1:*expr.Expr_IdentExpr#,
// Accumulator
@result,
// Init
true^#5:*expr.Constant_BoolValue#,
// LoopCondition
@not_strictly_false(
    @result^#6:*expr.Expr_IdentExpr#
)^#7:*expr.Expr_CallExpr#,
// LoopStep
_&&_(
    @result^#8:*expr.Expr_IdentExpr#,
    f^#4:*expr.Expr_IdentExpr#
)^#9:*expr.Expr_CallExpr#,
// Result
@result^#10:*expr.Expr_IdentExpr#)^#11:*expr.Expr_ComprehensionExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "m.existsOne(v, f)",
                p: "__comprehension__(
// Variable
v,
// Target
m^#1:*expr.Expr_IdentExpr#,
// Accumulator
@result,
// Init
0^#5:*expr.Constant_Int64Value#,
// LoopCondition
true^#6:*expr.Constant_BoolValue#,
// LoopStep
_?_:_(
    f^#4:*expr.Expr_IdentExpr#,
    _+_(
        @result^#7:*expr.Expr_IdentExpr#,
        1^#8:*expr.Constant_Int64Value#
    )^#9:*expr.Expr_CallExpr#,
    @result^#10:*expr.Expr_IdentExpr#
)^#11:*expr.Expr_CallExpr#,
// Result
_==_(
    @result^#12:*expr.Expr_IdentExpr#,
    1^#13:*expr.Constant_Int64Value#
)^#14:*expr.Expr_CallExpr#)^#15:*expr.Expr_ComprehensionExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "m.map(v, f)",
                p: "__comprehension__(
// Variable
v,
// Target
m^#1:*expr.Expr_IdentExpr#,
// Accumulator
@result,
// Init
[]^#5:*expr.Expr_ListExpr#,
// LoopCondition
true^#6:*expr.Constant_BoolValue#,
// LoopStep
_+_(
    @result^#7:*expr.Expr_IdentExpr#,
    [
        f^#4:*expr.Expr_IdentExpr#
    ]^#8:*expr.Expr_ListExpr#
)^#9:*expr.Expr_CallExpr#,
// Result
@result^#10:*expr.Expr_IdentExpr#)^#11:*expr.Expr_ComprehensionExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "m.map(v, p, f)",
                p: "__comprehension__(
// Variable
v,
// Target
m^#1:*expr.Expr_IdentExpr#,
// Accumulator
@result,
// Init
[]^#6:*expr.Expr_ListExpr#,
// LoopCondition
true^#7:*expr.Constant_BoolValue#,
// LoopStep
_?_:_(
    p^#4:*expr.Expr_IdentExpr#,
    _+_(
        @result^#8:*expr.Expr_IdentExpr#,
        [
            f^#5:*expr.Expr_IdentExpr#
        ]^#9:*expr.Expr_ListExpr#
    )^#10:*expr.Expr_CallExpr#,
    @result^#11:*expr.Expr_IdentExpr#
)^#12:*expr.Expr_CallExpr#,
// Result
@result^#13:*expr.Expr_IdentExpr#)^#14:*expr.Expr_ComprehensionExpr#",
                e: "",
                ..Default::default()
            },
            TestInfo {
                i: "m.filter(v, p)",
                p: "__comprehension__(
// Variable
v,
// Target
m^#1:*expr.Expr_IdentExpr#,
// Accumulator
@result,
// Init
[]^#5:*expr.Expr_ListExpr#,
// LoopCondition
true^#6:*expr.Constant_BoolValue#,
// LoopStep
_?_:_(
    p^#4:*expr.Expr_IdentExpr#,
    _+_(
        @result^#7:*expr.Expr_IdentExpr#,
        [
            v^#3:*expr.Expr_IdentExpr#
        ]^#8:*expr.Expr_ListExpr#
    )^#9:*expr.Expr_CallExpr#,
    @result^#10:*expr.Expr_IdentExpr#
)^#11:*expr.Expr_CallExpr#,
// Result
@result^#12:*expr.Expr_IdentExpr#)^#13:*expr.Expr_ComprehensionExpr#",
                e: "",
                ..Default::default()
            },
            // Parse error tests
            TestInfo {
                i: "0xFFFFFFFFFFFFFFFFF",
                p: "",
                e: "ERROR: <input>:1:1: invalid int literal
| 0xFFFFFFFFFFFFFFFFF
| ^",
                ..Default::default()
            },
            TestInfo {
                i: "0xFFFFFFFFFFFFFFFFFu",
                p: "",
                e: "ERROR: <input>:1:1: invalid uint literal
| 0xFFFFFFFFFFFFFFFFFu
| ^",
                ..Default::default()
            },
            TestInfo {
                i: "1.99e90000009",
                p: "",
                e: "ERROR: <input>:1:1: invalid double literal
| 1.99e90000009
| ^",
                ..Default::default()
            },
            TestInfo {
                i: "{",
                p: "",
                e: "ERROR: <input>:1:2: Syntax error: mismatched input '<EOF>' expecting '}'
| {
| .^",
                ..Default::default()
            },
            TestInfo {
                i: "*@a | b",
                p: "",
                e: "ERROR: <input>:1:1: Syntax error: mismatched input '*' expecting expression
| *@a | b
| ^
ERROR: <input>:1:2: unexpected character
| *@a | b
| .^
ERROR: <input>:1:5: unexpected single '|', expected '||'
| *@a | b
| ....^",
                ..Default::default()
            },
            TestInfo {
                i: "a | b",
                p: "",
                e: "ERROR: <input>:1:3: unexpected single '|', expected '||'
| a | b
| ..^",
                ..Default::default()
            },
            TestInfo {
                i: "a.?b && a[?b]",
                p: "",
                e: "ERROR: <input>:1:2: unsupported syntax '.?'
| a.?b && a[?b]
| .^
ERROR: <input>:1:10: unsupported syntax '[?'
| a.?b && a[?b]
| .........^",
                enable_optional_syntax: false,
            },
            TestInfo {
                i: "a.?b[?0] && a[?c]",
                p: r#"_&&_(
    _[?_](
        _?._(
            a^#1:*expr.Expr_IdentExpr#,
            "b"^#2:*expr.Constant_StringValue#
        )^#3:*expr.Expr_CallExpr#,
        0^#5:*expr.Constant_Int64Value#
    )^#4:*expr.Expr_CallExpr#,
    _[?_](
        a^#6:*expr.Expr_IdentExpr#,
        c^#8:*expr.Expr_IdentExpr#
    )^#7:*expr.Expr_CallExpr#
)^#9:*expr.Expr_CallExpr#"#,
                e: "",
                enable_optional_syntax: true,
            },
            TestInfo {
                i: "{?'key': value}",
                p: r#"{
    ?"key"^#3:*expr.Constant_StringValue#:value^#4:*expr.Expr_IdentExpr#^#2:*expr.Expr_CreateStruct_Entry#
}^#1:*expr.Expr_StructExpr#"#,
                e: "",
                enable_optional_syntax: true,
            },
            TestInfo {
                i: "[?a, ?b]",
                p: r#"[
    a^#2:*expr.Expr_IdentExpr#,
    b^#3:*expr.Expr_IdentExpr#
]^#1:*expr.Expr_ListExpr#"#,
                e: "",
                enable_optional_syntax: true,
            },
            TestInfo {
                i: "[?a[?b]]",
                p: r#"[
    _[?_](
        a^#2:*expr.Expr_IdentExpr#,
        b^#4:*expr.Expr_IdentExpr#
    )^#3:*expr.Expr_CallExpr#
]^#1:*expr.Expr_ListExpr#"#,
                e: "",
                enable_optional_syntax: true,
            },
            TestInfo {
                i: "[?a, ?b]",
                p: "",
                e: "ERROR: <input>:1:2: unsupported syntax '?'
| [?a, ?b]
| .^
ERROR: <input>:1:6: unsupported syntax '?'
| [?a, ?b]
| .....^",
                enable_optional_syntax: false,
            },
            TestInfo {
                i: "Msg{?field: value}",
                p: r#"Msg{
    ?field:value^#3:*expr.Expr_IdentExpr#^#2:*expr.Expr_CreateStruct_Entry#
}^#1:*expr.Expr_StructExpr#"#,
                e: "",
                enable_optional_syntax: true,
            },
            TestInfo {
                i: "Msg{?field: value} && {?'key': value}",
                p: "",
                e: "ERROR: <input>:1:5: unsupported syntax '?'
| Msg{?field: value} && {?'key': value}
| ....^
ERROR: <input>:1:24: unsupported syntax '?'
| Msg{?field: value} && {?'key': value}
| .......................^",
                enable_optional_syntax: false,
            },
            TestInfo {
                i: "has(m)",
                p: "",
                e: "ERROR: <input>:1:5: invalid argument to has() macro
| has(m)
| ....^",
                ..Default::default()
            },
            TestInfo {
                i: "1.all(2, 3)",
                p: "",
                e: "ERROR: <input>:1:7: argument must be a simple name
| 1.all(2, 3)
| ......^",
                ..Default::default()
            },
            TestInfo {
                i: "foo(a,b,)",
                p: "",
                e: "ERROR: <input>:1:9: Syntax error: mismatched input ')' expecting expression
| foo(a,b,)
| ........^",
                ..Default::default()
            },
            TestInfo {
                i: "a b",
                p: "",
                e: "ERROR: <input>:1:3: Syntax error: mismatched input 'b' expecting <EOF>
| a b
| ..^",
                ..Default::default()
            },
            TestInfo {
                i: "a.",
                p: "",
                e: "ERROR: <input>:1:3: Syntax error: mismatched input '<EOF>' expecting identifier
| a.
| ..^",
                ..Default::default()
            },
            TestInfo {
                i: "[1 2]",
                p: "",
                e: "ERROR: <input>:1:4: Syntax error: mismatched input '2' expecting ']'
| [1 2]
| ...^",
                ..Default::default()
            },
            TestInfo {
                i: "a ? b",
                p: "",
                e: "ERROR: <input>:1:6: Syntax error: mismatched input '<EOF>' expecting ':'
| a ? b
| .....^",
                ..Default::default()
            },
            TestInfo {
                i: "'unterminated",
                p: "",
                e: "ERROR: <input>:1:1: unterminated string literal
| 'unterminated
| ^",
                ..Default::default()
            },
        ];

        for test_case in test_cases {
            let parser = Parser::new().enable_optional_syntax(test_case.enable_optional_syntax);
            let result = parser.parse(test_case.i);
            if !test_case.p.is_empty() {
                assert_eq!(
                    to_go_like_string(result.as_ref().expect("Expected an AST")),
                    test_case.p,
                    "Expr `{}` failed",
                    test_case.i
                );
            }

            if !test_case.e.is_empty() {
                assert_eq!(
                    format!("{}", result.as_ref().expect_err("Expected an Err!")),
                    test_case.e,
                    "Error on `{}` failed",
                    test_case.i
                )
            }
        }
    }

    fn to_go_like_string(expr: &IdedExpr) -> String {
        let mut writer = DebugWriter::default();
        writer.buffer(expr);
        writer.done()
    }

    struct DebugWriter {
        buffer: String,
        indents: usize,
        line_start: bool,
    }

    impl Default for DebugWriter {
        fn default() -> Self {
            Self {
                buffer: String::default(),
                indents: 0,
                line_start: true,
            }
        }
    }

    impl DebugWriter {
        fn buffer(&mut self, expr: &IdedExpr) -> &Self {
            let e = match &expr.expr {
                Expr::Unspecified => "UNSPECIFIED!",
                Expr::Call(call) => {
                    if let Some(target) = &call.target {
                        self.buffer(target);
                        self.push(".");
                    }
                    self.push(call.func_name.as_str());
                    self.push("(");
                    if !call.args.is_empty() {
                        self.inc_indent();
                        self.newline();
                        for i in 0..call.args.len() {
                            if i > 0 {
                                self.push(",");
                                self.newline();
                            }
                            self.buffer(&call.args[i]);
                        }
                        self.dec_indent();
                        self.newline();
                    }
                    self.push(")");
                    &format!("^#{}:{}#", expr.id, "*expr.Expr_CallExpr")
                }
                Expr::Comprehension(comprehension) => {
                    self.push("__comprehension__(\n");
                    self.push_comprehension(comprehension);
                    &format!(")^#{}:{}#", expr.id, "*expr.Expr_ComprehensionExpr")
                }
                Expr::Ident(id) => &format!("{}^#{}:{}#", id, expr.id, "*expr.Expr_IdentExpr"),
                Expr::List(list) => {
                    self.push("[");
                    if !list.elements.is_empty() {
                        self.inc_indent();
                        self.newline();
                        for (i, element) in list.elements.iter().enumerate() {
                            if i > 0 {
                                self.push(",");
                                self.newline();
                            }
                            self.buffer(element);
                        }
                        self.dec_indent();
                        self.newline();
                    }
                    self.push("]");
                    &format!("^#{}:{}#", expr.id, "*expr.Expr_ListExpr")
                }
                Expr::Literal(val) => match val {
                    LiteralValue::String(s) => &format!(
                        "\"{}\"^#{}:{}#",
                        s.inner(),
                        expr.id,
                        "*expr.Constant_StringValue"
                    ),
                    LiteralValue::Boolean(b) => {
                        &format!("{}^#{}:{}#", b.inner(), expr.id, "*expr.Constant_BoolValue")
                    }
                    LiteralValue::Int(i) => &format!(
                        "{}^#{}:{}#",
                        i.inner(),
                        expr.id,
                        "*expr.Constant_Int64Value"
                    ),
                    LiteralValue::UInt(u) => &format!(
                        "{}u^#{}:{}#",
                        u.inner(),
                        expr.id,
                        "*expr.Constant_Uint64Value"
                    ),
                    LiteralValue::Double(f) => &format!(
                        "{}^#{}:{}#",
                        f.inner(),
                        expr.id,
                        "*expr.Constant_DoubleValue"
                    ),
                    LiteralValue::Bytes(bytes) => &format!(
                        "b\"{}\"^#{}:{}#",
                        String::from_utf8_lossy(bytes),
                        expr.id,
                        "*expr.Constant_BytesValue"
                    ),
                    LiteralValue::Null => {
                        &format!("null^#{}:{}#", expr.id, "*expr.Constant_NullValue")
                    }
                },
                Expr::Map(map) => {
                    self.push("{");
                    self.inc_indent();
                    if !map.entries.is_empty() {
                        self.newline();
                    }
                    for (i, entry) in map.entries.iter().enumerate() {
                        match &entry.expr {
                            EntryExpr::StructField(_) => panic!("WAT?!"),
                            EntryExpr::MapEntry(e) => {
                                if e.optional {
                                    self.push("?");
                                }
                                self.buffer(&e.key);
                                self.push(":");
                                self.buffer(&e.value);
                                self.push(&format!(
                                    "^#{}:{}#",
                                    entry.id, "*expr.Expr_CreateStruct_Entry"
                                ));
                            }
                        }
                        if i < map.entries.len() - 1 {
                            self.push(",");
                        }
                        self.newline();
                    }
                    self.dec_indent();
                    self.push("}");
                    &format!("^#{}:{}#", expr.id, "*expr.Expr_StructExpr")
                }
                Expr::Select(select) => {
                    self.buffer(&select.operand);
                    let suffix = if select.test { "~test-only~" } else { "" };

                    &format!(
                        ".{}{}^#{}:{}#",
                        select.field, suffix, expr.id, "*expr.Expr_SelectExpr"
                    )
                }
                Expr::Struct(s) => {
                    self.push(&s.type_name);
                    self.push("{");
                    self.inc_indent();
                    if !s.entries.is_empty() {
                        self.newline();
                    }
                    for (i, entry) in s.entries.iter().enumerate() {
                        match &entry.expr {
                            EntryExpr::StructField(field) => {
                                if field.optional {
                                    self.push("?");
                                }
                                self.push(&field.field);
                                self.push(":");
                                self.buffer(&field.value);
                                self.push(&format!(
                                    "^#{}:{}#",
                                    entry.id, "*expr.Expr_CreateStruct_Entry"
                                ));
                            }
                            EntryExpr::MapEntry(_) => panic!("WAT?!"),
                        }
                        if i < s.entries.len() - 1 {
                            self.push(",");
                        }
                        self.newline();
                    }
                    self.dec_indent();
                    self.push("}");
                    &format!("^#{}:{}#", expr.id, "*expr.Expr_StructExpr")
                }
            };
            self.push(e);
            self
        }

        fn push(&mut self, literal: &str) {
            self.indent();
            self.buffer.push_str(literal);
        }

        fn indent(&mut self) {
            if self.line_start {
                self.line_start = false;
                self.buffer.push_str(
                    iter::repeat_n("    ", self.indents)
                        .collect::<String>()
                        .as_str(),
                )
            }
        }

        fn newline(&mut self) {
            self.buffer.push('\n');
            self.line_start = true;
        }

        fn inc_indent(&mut self) {
            self.indents += 1;
        }

        fn dec_indent(&mut self) {
            self.indents -= 1;
        }

        fn done(self) -> String {
            self.buffer
        }

        fn push_comprehension(&mut self, comprehension: &ComprehensionExpr) {
            self.push("// Variable\n");
            self.push(comprehension.iter_var.as_str());
            self.push(",\n");
            self.push("// Target\n");
            self.buffer(&comprehension.iter_range);
            self.push(",\n");
            self.push("// Accumulator\n");
            self.push(comprehension.accu_var.as_str());
            self.push(",\n");
            self.push("// Init\n");
            self.buffer(&comprehension.accu_init);
            self.push(",\n");
            self.push("// LoopCondition\n");
            self.buffer(&comprehension.loop_cond);
            self.push(",\n");
            self.push("// LoopStep\n");
            self.buffer(&comprehension.loop_step);
            self.push(",\n");
            self.push("// Result\n");
            self.buffer(&comprehension.result);
        }
    }
}
