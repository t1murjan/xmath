//! Tokenizer and recursive-descent parser for the linear math syntax.
//!
//! ```text
//! x := 5 m                 definition
//! f(x) := x^2 + 1          function definition
//! g ≡ 9.81 m/s^2           global definition (also `::=`)
//! i := 0..10               range variable (also `0, 0.5..10`)
//! v[i] := i^2              indexed definition
//! f(3) =                   evaluation
//! F = kN                   evaluation shown in the given unit
//! ```

use crate::ast::{BinOp, Expr, PStmt, Stmt, Target, UnOp};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub msg: String,
    /// Byte offset in the source where the problem was found.
    pub pos: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Imag(f64),
    Ident(String),
    Str(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    LBrack,
    RBrack,
    Comma,
    Semi,
    LBrace,
    RBrace,
    Arrow,
    In,
    Define,
    GlobalDefine,
    Eq,
    EqEq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    DotDot,
    Bang,
    Bar,
    And,
    Or,
    Not,
    Sqrt,
    Eof,
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Tok::Num(x) => format!("число {x}"),
            Tok::Imag(x) => format!("число {x}i"),
            Tok::Ident(s) => format!("'{s}'"),
            Tok::Str(_) => "строка".into(),
            Tok::Eof => "конец выражения".into(),
            other => format!("'{}'", tok_text(other)),
        }
    }
}

fn tok_text(t: &Tok) -> &'static str {
    match t {
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::Slash => "/",
        Tok::Caret => "^",
        Tok::LParen => "(",
        Tok::RParen => ")",
        Tok::LBrack => "[",
        Tok::RBrack => "]",
        Tok::Comma => ",",
        Tok::Semi => ";",
        Tok::LBrace => "{",
        Tok::RBrace => "}",
        Tok::Arrow => "←",
        Tok::In => "∈",
        Tok::Define => ":=",
        Tok::GlobalDefine => "≡",
        Tok::Eq => "=",
        Tok::EqEq => "==",
        Tok::Ne => "≠",
        Tok::Lt => "<",
        Tok::Gt => ">",
        Tok::Le => "≤",
        Tok::Ge => "≥",
        Tok::DotDot => "..",
        Tok::Bang => "!",
        Tok::Bar => "|",
        Tok::And => "and",
        Tok::Or => "or",
        Tok::Not => "¬",
        Tok::Sqrt => "√",
        _ => "?",
    }
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    pos: usize,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '°' || c == '∞' || c == '%'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let at = |i: usize| chars.get(i).map(|c| c.1).unwrap_or('\0');
    while i < chars.len() {
        let (pos, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && at(i + 1).is_ascii_digit()) {
            let start = i;
            while at(i).is_ascii_digit() {
                i += 1;
            }
            if at(i) == '.' && at(i + 1) != '.' {
                i += 1;
                while at(i).is_ascii_digit() {
                    i += 1;
                }
            }
            if (at(i) == 'e' || at(i) == 'E')
                && (at(i + 1).is_ascii_digit()
                    || ((at(i + 1) == '+' || at(i + 1) == '-') && at(i + 2).is_ascii_digit()))
            {
                i += 2;
                while at(i).is_ascii_digit() {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().map(|c| c.1).collect();
            let value: f64 = text.parse().map_err(|_| ParseError {
                msg: format!("неверное число '{text}'"),
                pos,
            })?;
            if (at(i) == 'i' || at(i) == 'j') && !is_ident_continue(at(i + 1)) {
                i += 1;
                toks.push(Token { tok: Tok::Imag(value), pos });
            } else {
                toks.push(Token { tok: Tok::Num(value), pos });
            }
            continue;
        }
        if is_ident_start(c) {
            let start = i;
            i += 1;
            if c != '%' && c != '∞' {
                loop {
                    let n = at(i);
                    if is_ident_continue(n) {
                        i += 1;
                    } else if n == '.' && at(i + 1).is_alphanumeric() {
                        // literal subscript, e.g. `x.max`
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            // difference units `Δ°C`, `Δ°F` (the degree sign is not an identifier char)
            if i == start + 1 && c == 'Δ' && at(i) == '°' && at(i + 1).is_alphabetic() {
                i += 1;
                while is_ident_continue(at(i)) {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().map(|c| c.1).collect();
            let tok = match text.as_str() {
                "and" => Tok::And,
                "or" => Tok::Or,
                _ => Tok::Ident(text),
            };
            toks.push(Token { tok, pos });
            continue;
        }
        if c == '"' {
            let start = i + 1;
            i += 1;
            while i < chars.len() && at(i) != '"' {
                i += 1;
            }
            if i >= chars.len() {
                return Err(ParseError { msg: "незакрытая строка".into(), pos });
            }
            let text: String = chars[start..i].iter().map(|c| c.1).collect();
            i += 1;
            toks.push(Token { tok: Tok::Str(text), pos });
            continue;
        }
        let two = (c, at(i + 1));
        let (tok, len) = match two {
            (':', ':') if at(i + 2) == '=' => (Tok::GlobalDefine, 3),
            (':', '=') => (Tok::Define, 2),
            ('=', '=') => (Tok::EqEq, 2),
            ('!', '=') => (Tok::Ne, 2),
            ('<', '=') => (Tok::Le, 2),
            ('>', '=') => (Tok::Ge, 2),
            ('<', '>') => (Tok::Ne, 2),
            ('.', '.') => (Tok::DotDot, 2),
            ('*', '*') => (Tok::Caret, 2),
            (':', _) => (Tok::Define, 1),
            ('≔', _) => (Tok::Define, 1),
            ('<', '-') => (Tok::Arrow, 2),
            ('←', _) => (Tok::Arrow, 1),
            ('∈', _) => (Tok::In, 1),
            ('{', _) => (Tok::LBrace, 1),
            ('}', _) => (Tok::RBrace, 1),
            ('≡', _) => (Tok::GlobalDefine, 1),
            ('=', _) => (Tok::Eq, 1),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) | ('−', _) => (Tok::Minus, 1),
            ('*', _) | ('·', _) | ('×', _) | ('⋅', _) => (Tok::Star, 1),
            ('/', _) | ('÷', _) => (Tok::Slash, 1),
            ('^', _) => (Tok::Caret, 1),
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            ('[', _) => (Tok::LBrack, 1),
            (']', _) => (Tok::RBrack, 1),
            (',', _) => (Tok::Comma, 1),
            (';', _) => (Tok::Semi, 1),
            ('<', _) => (Tok::Lt, 1),
            ('>', _) => (Tok::Gt, 1),
            ('≤', _) => (Tok::Le, 1),
            ('≥', _) => (Tok::Ge, 1),
            ('≠', _) => (Tok::Ne, 1),
            ('!', _) => (Tok::Bang, 1),
            ('|', _) => (Tok::Bar, 1),
            ('∧', _) | ('&', _) => (Tok::And, 1),
            ('∨', _) => (Tok::Or, 1),
            ('¬', _) => (Tok::Not, 1),
            ('√', _) => (Tok::Sqrt, 1),
            ('…', _) => (Tok::DotDot, 1),
            ('■', _) => {
                return Err(ParseError { msg: "Заполните поле ввода ■".into(), pos });
            }
            _ => {
                return Err(ParseError { msg: format!("недопустимый символ '{c}'"), pos });
            }
        };
        toks.push(Token { tok, pos });
        i += len;
    }
    toks.push(Token { tok: Tok::Eof, pos: src.len() });
    Ok(toks)
}

struct Parser {
    toks: Vec<Token>,
    i: usize,
}

type PResult<T> = Result<T, ParseError>;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.i].tok
    }

    fn pos(&self) -> usize {
        self.toks[self.i].pos
    }

    fn next(&mut self) -> Tok {
        let t = self.toks[self.i].tok.clone();
        if self.i + 1 < self.toks.len() {
            self.i += 1;
        }
        t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == t {
            self.next();
            true
        } else {
            false
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(ParseError { msg: msg.into(), pos: self.pos() })
    }

    fn expect(&mut self, t: Tok) -> PResult<()> {
        if self.eat(&t) {
            Ok(())
        } else {
            self.err(format!("ожидалось '{}', найдено {}", tok_text(&t), self.peek().describe()))
        }
    }

    fn expr(&mut self) -> PResult<Expr> {
        let mut a = self.and_expr()?;
        while self.eat(&Tok::Or) {
            let b = self.and_expr()?;
            a = Expr::Binary(BinOp::Or, Box::new(a), Box::new(b));
        }
        Ok(a)
    }

    fn and_expr(&mut self) -> PResult<Expr> {
        let mut a = self.not_expr()?;
        while self.eat(&Tok::And) {
            let b = self.not_expr()?;
            a = Expr::Binary(BinOp::And, Box::new(a), Box::new(b));
        }
        Ok(a)
    }

    fn not_expr(&mut self) -> PResult<Expr> {
        if self.eat(&Tok::Not) {
            let a = self.not_expr()?;
            return Ok(Expr::Unary(UnOp::Not, Box::new(a)));
        }
        self.cmp_expr()
    }

    fn cmp_expr(&mut self) -> PResult<Expr> {
        let mut a = self.add_expr()?;
        loop {
            let op = match self.peek() {
                Tok::EqEq => BinOp::Eq,
                Tok::Ne => BinOp::Ne,
                Tok::Lt => BinOp::Lt,
                Tok::Gt => BinOp::Gt,
                Tok::Le => BinOp::Le,
                Tok::Ge => BinOp::Ge,
                _ => return Ok(a),
            };
            self.next();
            let b = self.add_expr()?;
            a = Expr::Binary(op, Box::new(a), Box::new(b));
        }
    }

    fn add_expr(&mut self) -> PResult<Expr> {
        let mut a = self.mul_expr()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(a),
            };
            self.next();
            let b = self.mul_expr()?;
            a = Expr::Binary(op, Box::new(a), Box::new(b));
        }
    }

    fn starts_primary(&self) -> bool {
        match self.peek() {
            Tok::Ident(s) => !matches!(s.as_str(), "if" | "else" | "otherwise" | "on"),
            t => matches!(t, Tok::Num(_) | Tok::Imag(_) | Tok::LParen | Tok::Sqrt),
        }
    }

    fn mul_expr(&mut self) -> PResult<Expr> {
        let mut a = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => {
                    self.next();
                    BinOp::Mul
                }
                Tok::Slash => {
                    self.next();
                    BinOp::Div
                }
                // implicit multiplication: `2 m`, `3(x + 1)`
                _ if self.starts_primary() => BinOp::Mul,
                _ => return Ok(a),
            };
            let b = self.unary()?;
            a = Expr::Binary(op, Box::new(a), Box::new(b));
        }
    }

    fn unary(&mut self) -> PResult<Expr> {
        if self.eat(&Tok::Minus) {
            let a = self.unary()?;
            return Ok(Expr::Unary(UnOp::Neg, Box::new(a)));
        }
        if self.eat(&Tok::Plus) {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> PResult<Expr> {
        let base = self.postfix()?;
        if self.eat(&Tok::Caret) {
            let exp = self.unary()?;
            return Ok(Expr::Binary(BinOp::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Tok::Bang => {
                    self.next();
                    e = Expr::Unary(UnOp::Factorial, Box::new(e));
                }
                Tok::LBrack => {
                    self.next();
                    let idx = self.list(Tok::RBrack)?;
                    if idx.is_empty() {
                        return self.err("пустой индекс");
                    }
                    e = Expr::Index(Box::new(e), idx);
                }
                _ => return Ok(e),
            }
        }
    }

    /// Comma separated expressions up to (and including) `close`.
    fn list(&mut self, close: Tok) -> PResult<Vec<Expr>> {
        let mut items = Vec::new();
        if self.eat(&close) {
            return Ok(items);
        }
        loop {
            items.push(self.expr_oe()?);
            if self.eat(&Tok::Comma) {
                continue;
            }
            self.expect(close)?;
            return Ok(items);
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        let pos = self.pos();
        match self.next() {
            Tok::Num(x) => Ok(Expr::Num(x)),
            Tok::Imag(x) => Ok(Expr::Imag(x)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Ident(name) => {
                if self.eat(&Tok::LParen) {
                    let args = self.list(Tok::RParen)?;
                    Ok(Expr::Call(name, args))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Tok::LBrace => self.program(pos),
            Tok::LParen => {
                let e = self.expr_oe()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Tok::LBrack => {
                let mut rows = vec![];
                let mut row = vec![];
                if self.eat(&Tok::RBrack) {
                    return Err(ParseError { msg: "пустая матрица".into(), pos });
                }
                loop {
                    row.push(self.expr_oe()?);
                    match self.next() {
                        Tok::Comma => {}
                        Tok::Semi => rows.push(std::mem::take(&mut row)),
                        Tok::RBrack => {
                            rows.push(row);
                            break;
                        }
                        t => {
                            return self.err(format!("ожидалось ',', ';' или ']', найдено {}", t.describe()))
                        }
                    }
                }
                let n = rows[0].len();
                if rows.iter().any(|r| r.len() != n) {
                    return Err(ParseError { msg: "строки матрицы имеют разную длину".into(), pos });
                }
                Ok(Expr::Matrix(rows))
            }
            Tok::Bar => {
                let e = self.expr_oe()?;
                self.expect(Tok::Bar)?;
                Ok(Expr::Abs(Box::new(e)))
            }
            Tok::Sqrt => {
                let e = self.power()?;
                Ok(Expr::Call("sqrt".into(), vec![e]))
            }
            Tok::Eof => Err(ParseError { msg: "незаконченное выражение".into(), pos }),
            t => Err(ParseError { msg: format!("неожиданно: {}", t.describe()), pos }),
        }
    }

    // ------------------------------------------------------------ programs

    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == kw)
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.next();
            true
        } else {
            false
        }
    }

    fn peek2(&self) -> &Tok {
        &self.toks[(self.i + 1).min(self.toks.len() - 1)].tok
    }

    /// `expr [on error expr]...`
    fn expr_oe(&mut self) -> PResult<Expr> {
        let mut a = self.expr()?;
        while self.is_kw("on") && matches!(self.peek2(), Tok::Ident(s) if s == "error") {
            self.next();
            self.next();
            let b = self.expr()?;
            a = Expr::OnError(Box::new(a), Box::new(b));
        }
        Ok(a)
    }

    /// Statements up to and including the closing `}` (the `{` is already consumed).
    fn block_body(&mut self) -> PResult<Vec<PStmt>> {
        let mut out = Vec::new();
        loop {
            while self.eat(&Tok::Semi) {}
            if self.eat(&Tok::RBrace) {
                return Ok(out);
            }
            let (s, braced) = self.pstmt()?;
            out.push(s);
            if self.eat(&Tok::Semi) || braced || *self.peek() == Tok::RBrace {
                continue;
            }
            return self.err(format!("ожидалось ';' или '}}', найдено {}", self.peek().describe()));
        }
    }

    fn block(&mut self) -> PResult<Vec<PStmt>> {
        self.expect(Tok::LBrace)?;
        self.block_body()
    }

    fn program(&mut self, pos: usize) -> PResult<Expr> {
        let stmts = self.block_body()?;
        if stmts.is_empty() {
            return Err(ParseError { msg: "пустая программа".into(), pos });
        }
        Ok(Expr::Program(stmts))
    }

    /// `if cond { .. } [else if .. | else { .. } | otherwise { .. }]`, the `if` keyword is current.
    fn if_block(&mut self) -> PResult<PStmt> {
        self.next();
        let cond = self.expr_oe()?;
        let then = self.block()?;
        let else_ = if self.eat_kw("otherwise") {
            Some(self.block()?)
        } else if self.eat_kw("else") {
            if self.is_kw("if") {
                Some(vec![self.if_block()?])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(PStmt::If { cond, then, else_ })
    }

    /// One program statement; the flag tells whether it ended with a `}` block.
    fn pstmt(&mut self) -> PResult<(PStmt, bool)> {
        let call_follows = matches!(self.peek2(), Tok::LParen);
        if let Tok::Ident(kw) = self.peek().clone() {
            match kw.as_str() {
                "if" => {
                    let save = self.i;
                    match self.if_block() {
                        Ok(s) => return Ok((s, true)),
                        // `if(a, b, c)` is the function
                        Err(e) => {
                            if !call_follows {
                                return Err(e);
                            }
                            self.i = save;
                        }
                    }
                }
                "for" if !call_follows => {
                    self.next();
                    let var = match self.next() {
                        Tok::Ident(v) => v,
                        t => return self.err(format!("ожидалось имя переменной цикла, найдено {}", t.describe())),
                    };
                    if !(self.eat(&Tok::In) || self.eat_kw("in")) {
                        return self.err(format!("ожидалось '∈' или 'in', найдено {}", self.peek().describe()));
                    }
                    let iter = self.range_or_expr()?;
                    let body = self.block()?;
                    return Ok((PStmt::For { var, iter, body }, true));
                }
                "while" if !call_follows => {
                    self.next();
                    let cond = self.expr_oe()?;
                    let body = self.block()?;
                    return Ok((PStmt::While { cond, body }, true));
                }
                _ => {}
            }
        }
        let simple = if self.is_kw("break") && !call_follows {
            self.next();
            PStmt::Break
        } else if self.is_kw("continue") && !call_follows {
            self.next();
            PStmt::Continue
        } else if self.is_kw("return") && !call_follows {
            self.next();
            PStmt::Return(self.expr_oe()?)
        } else {
            let lhs_pos = self.pos();
            let lhs = self.expr_oe()?;
            if matches!(self.peek(), Tok::Arrow | Tok::Define) {
                self.next();
                let target = match to_target(lhs) {
                    Ok(t @ (Target::Var(_) | Target::Indexed(..))) => t,
                    Ok(_) => {
                        return Err(ParseError {
                            msg: "в программе можно присваивать только переменным и элементам векторов".into(),
                            pos: lhs_pos,
                        })
                    }
                    Err(msg) => return Err(ParseError { msg, pos: lhs_pos }),
                };
                PStmt::Assign(target, self.expr_oe()?)
            } else {
                PStmt::Expr(lhs)
            }
        };
        if self.eat_kw("if") {
            let cond = self.expr_oe()?;
            return Ok((PStmt::If { cond, then: vec![simple], else_: None }, false));
        }
        Ok((simple, false))
    }

    /// `a..b`, `a, b..c` or a plain expression.
    fn range_or_expr(&mut self) -> PResult<Expr> {
        let first = self.expr_oe()?;
        if self.eat(&Tok::Comma) {
            let second = self.expr()?;
            self.expect(Tok::DotDot)?;
            let last = self.expr()?;
            return Ok(Expr::Range(Box::new(first), Some(Box::new(second)), Box::new(last)));
        }
        if self.eat(&Tok::DotDot) {
            let last = self.expr()?;
            return Ok(Expr::Range(Box::new(first), None, Box::new(last)));
        }
        Ok(first)
    }

    fn statement(&mut self) -> PResult<Stmt> {
        let lhs_pos = self.pos();
        let lhs = self.range_or_expr()?;
        let stmt = match self.peek() {
            Tok::Define | Tok::GlobalDefine | Tok::Arrow => {
                let global = self.next() == Tok::GlobalDefine;
                let target = to_target(lhs).map_err(|msg| ParseError { msg, pos: lhs_pos })?;
                let expr = self.range_or_expr()?;
                Stmt::Define { target, expr, global }
            }
            Tok::Eq => {
                self.next();
                if matches!(lhs, Expr::Range(..)) {
                    return Err(ParseError { msg: "диапазон нельзя вычислить напрямую".into(), pos: lhs_pos });
                }
                let unit = if *self.peek() == Tok::Eof { None } else { Some(self.expr()?) };
                Stmt::Eval { expr: lhs, unit }
            }
            _ => Stmt::Show(lhs),
        };
        if *self.peek() != Tok::Eof {
            return self.err(format!("неожиданно: {}", self.peek().describe()));
        }
        Ok(stmt)
    }
}

fn to_target(e: Expr) -> Result<Target, String> {
    match e {
        Expr::Var(n) => Ok(Target::Var(n)),
        Expr::Call(n, args) => {
            let mut params = Vec::new();
            for a in args {
                match a {
                    Expr::Var(p) => params.push(p),
                    other => return Err(format!("параметр функции должен быть именем, найдено '{other}'")),
                }
            }
            Ok(Target::Func(n, params))
        }
        Expr::Index(base, idx) => match *base {
            Expr::Var(n) => Ok(Target::Indexed(n, idx)),
            other => Err(format!("нельзя присвоить значение '{other}'")),
        },
        other => Err(format!("нельзя определить '{other}': слева должно быть имя или функция")),
    }
}

pub fn parse_statement(src: &str) -> Result<Stmt, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, i: 0 };
    if *p.peek() == Tok::Eof {
        return p.err("пустая область");
    }
    p.statement()
}

pub fn parse_expr(src: &str) -> Result<Expr, ParseError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, i: 0 };
    let e = p.range_or_expr()?;
    if *p.peek() != Tok::Eof {
        return p.err(format!("неожиданно: {}", p.peek().describe()));
    }
    Ok(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(s: &str) -> String {
        parse_statement(s).unwrap().to_string()
    }

    #[test]
    fn statements() {
        assert_eq!(roundtrip("x:=5"), "x := 5");
        assert_eq!(roundtrip("x : 5"), "x := 5");
        assert_eq!(roundtrip("f(x, y) := x^2 + y"), "f(x, y) := x^2 + y");
        assert_eq!(roundtrip("g ≡ 9.81"), "g ≡ 9.81");
        assert_eq!(roundtrip("g ::= 9.81"), "g ≡ 9.81");
        assert_eq!(roundtrip("x ="), "x =");
        assert_eq!(roundtrip("F = kN"), "F = kN");
        assert_eq!(roundtrip("i := 0..10"), "i := 0..10");
        assert_eq!(roundtrip("x := 0, 0.1..1"), "x := 0, 0.1..1");
        assert_eq!(roundtrip("v[i] := i^2"), "v[i] := i^2");
    }

    #[test]
    fn precedence() {
        assert_eq!(parse_expr("-x^2").unwrap().to_string(), "-x^2");
        assert_eq!(parse_expr("(-x)^2").unwrap().to_string(), "(-x)^2");
        assert_eq!(parse_expr("2^3^2").unwrap().to_string(), "2^3^2");
        assert_eq!(parse_expr("a - (b - c)").unwrap().to_string(), "a - (b - c)");
        assert_eq!(parse_expr("a/(b*c)").unwrap().to_string(), "a/(b*c)");
        assert_eq!(parse_expr("5 kN").unwrap().to_string(), "5*kN");
        assert_eq!(parse_expr("2i + 3").unwrap().to_string(), "2i + 3");
        assert_eq!(parse_expr("2in").unwrap().to_string(), "2*in");
        assert_eq!(parse_expr("x.max + 1e-3").unwrap().to_string(), "x.max + 0.001");
        assert_eq!(parse_expr("[1, 2; 3, 4]").unwrap().to_string(), "[1, 2; 3, 4]");
        assert_eq!(parse_expr("|v|").unwrap().to_string(), "|v|");
        assert_eq!(parse_expr("√x + 1").unwrap().to_string(), "sqrt(x) + 1");
        assert_eq!(parse_expr("n!").unwrap().to_string(), "n!");
        assert_eq!(parse_expr("a ≤ b and c ≠ d").unwrap().to_string(), "a <= b and c != d");
    }

    #[test]
    fn errors() {
        assert!(parse_statement("x := ").is_err());
        assert!(parse_statement("2 := 3").is_err());
        assert!(parse_statement("(1 + 2").is_err());
        assert!(parse_statement("f(1) := 3").is_err());
    }
}
