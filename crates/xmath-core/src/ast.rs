//! Syntax tree of math regions.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
    Factorial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }

    /// Binding strength used for printing and parenthesisation.
    pub fn precedence(self) -> u8 {
        match self {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 3,
            BinOp::Add | BinOp::Sub => 4,
            BinOp::Mul | BinOp::Div => 5,
            BinOp::Pow => 7,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    /// Imaginary literal, e.g. `2i`.
    Imag(f64),
    Str(String),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Index(Box<Expr>, Vec<Expr>),
    /// Matrix literal, row-major: `[1, 2; 3, 4]`.
    Matrix(Vec<Vec<Expr>>),
    /// Range `first, second .. last` (second is optional).
    Range(Box<Expr>, Option<Box<Expr>>, Box<Expr>),
    /// `|x|`: absolute value, vector length or determinant.
    Abs(Box<Expr>),
    /// Mathcad program: `{ s ← 0; for i ∈ 1..n { s ← s + i }; s }`.
    Program(Vec<PStmt>),
    /// `expr1 on error expr2`: the value of `expr2` if `expr1` fails.
    OnError(Box<Expr>, Box<Expr>),
}

/// A statement inside a program block.
#[derive(Debug, Clone, PartialEq)]
pub enum PStmt {
    /// Local assignment `x ← e` or `v[i] ← e` (the target is `Target::Var` or `Target::Indexed`).
    Assign(Target, Expr),
    /// A bare expression (its value is the value of the program if it is the last statement).
    Expr(Expr),
    /// `if cond { then } otherwise { else_ }`; the postfix form `stmt if cond` is a single-statement `then`.
    If { cond: Expr, then: Vec<PStmt>, else_: Option<Vec<PStmt>> },
    /// `for var ∈ iter { body }`.
    For { var: String, iter: Expr, body: Vec<PStmt> },
    /// `while cond { body }`.
    While { cond: Expr, body: Vec<PStmt> },
    Break,
    Continue,
    Return(Expr),
}

impl PStmt {
    /// Statements that can take a postfix `if`.
    pub fn is_simple(&self) -> bool {
        matches!(self, PStmt::Assign(..) | PStmt::Expr(_) | PStmt::Break | PStmt::Continue | PStmt::Return(_))
    }
}

impl Expr {
    pub fn precedence(&self) -> u8 {
        match self {
            Expr::Range(..) | Expr::OnError(..) => 0,
            Expr::Binary(op, ..) => op.precedence(),
            Expr::Unary(UnOp::Not, _) => 3,
            Expr::Unary(UnOp::Neg, _) => 6,
            Expr::Unary(UnOp::Factorial, _) => 8,
            Expr::Num(x) if *x < 0.0 => 6,
            _ => 9,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Var(String),
    Func(String, Vec<String>),
    Indexed(String, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `target := expr` (or `≡` when `global`).
    Define { target: Target, expr: Expr, global: bool },
    /// `expr =` optionally followed by a display unit.
    Eval { expr: Expr, unit: Option<Expr> },
    /// A bare expression that is only displayed.
    Show(Expr),
}

fn fmt_num(f: &mut fmt::Formatter<'_>, x: f64) -> fmt::Result {
    if x.is_infinite() {
        write!(f, "∞")
    } else {
        write!(f, "{x}")
    }
}

fn write_child(f: &mut fmt::Formatter<'_>, e: &Expr, paren: bool) -> fmt::Result {
    if paren {
        write!(f, "({e})")
    } else {
        write!(f, "{e}")
    }
}

fn write_list(f: &mut fmt::Formatter<'_>, items: &[Expr]) -> fmt::Result {
    for (i, a) in items.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{a}")?;
    }
    Ok(())
}

/// Prints the expression back as parseable source text.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Num(x) => fmt_num(f, *x),
            Expr::Imag(x) => {
                fmt_num(f, *x)?;
                write!(f, "i")
            }
            Expr::Str(s) => write!(f, "\"{s}\""),
            Expr::Var(v) => write!(f, "{v}"),
            Expr::Unary(UnOp::Neg, a) => {
                write!(f, "-")?;
                write_child(f, a, a.precedence() < 6)
            }
            Expr::Unary(UnOp::Not, a) => {
                write!(f, "¬")?;
                write_child(f, a, a.precedence() < 4)
            }
            Expr::Unary(UnOp::Factorial, a) => {
                write_child(f, a, a.precedence() < 9)?;
                write!(f, "!")
            }
            Expr::Binary(op, a, b) => {
                let p = op.precedence();
                if *op == BinOp::Pow {
                    write_child(f, a, a.precedence() <= p)?;
                    write!(f, "^")?;
                    write_child(f, b, b.precedence() < p)
                } else {
                    write_child(f, a, a.precedence() < p)?;
                    match op {
                        BinOp::Mul | BinOp::Div => write!(f, "{}", op.symbol())?,
                        _ => write!(f, " {} ", op.symbol())?,
                    }
                    write_child(f, b, b.precedence() <= p)
                }
            }
            Expr::Call(name, args) => {
                write!(f, "{name}(")?;
                write_list(f, args)?;
                write!(f, ")")
            }
            Expr::Index(base, idx) => {
                write_child(f, base, base.precedence() < 9)?;
                write!(f, "[")?;
                write_list(f, idx)?;
                write!(f, "]")
            }
            Expr::Matrix(rows) => {
                write!(f, "[")?;
                for (i, r) in rows.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write_list(f, r)?;
                }
                write!(f, "]")
            }
            Expr::Range(a, b, c) => {
                write!(f, "{a}")?;
                if let Some(b) = b {
                    write!(f, ", {b}")?;
                }
                write!(f, "..{c}")
            }
            Expr::Abs(a) => write!(f, "|{a}|"),
            Expr::Program(stmts) => write_block(f, stmts),
            Expr::OnError(a, b) => {
                write_child(f, a, matches!(**a, Expr::Range(..)))?;
                write!(f, " on error ")?;
                write_child(f, b, b.precedence() == 0)
            }
        }
    }
}

fn write_block(f: &mut fmt::Formatter<'_>, stmts: &[PStmt]) -> fmt::Result {
    write!(f, "{{ ")?;
    for (i, s) in stmts.iter().enumerate() {
        if i > 0 {
            write!(f, "; ")?;
        }
        write!(f, "{s}")?;
    }
    write!(f, " }}")
}

impl fmt::Display for PStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PStmt::Assign(t, e) => write!(f, "{t} ← {e}"),
            PStmt::Expr(e) => write!(f, "{e}"),
            PStmt::If { cond, then, else_ } => {
                if else_.is_none() && then.len() == 1 && then[0].is_simple() {
                    return write!(f, "{} if {cond}", then[0]);
                }
                write!(f, "if {cond} ")?;
                write_block(f, then)?;
                if let Some(e) = else_ {
                    write!(f, " otherwise ")?;
                    write_block(f, e)?;
                }
                Ok(())
            }
            PStmt::For { var, iter, body } => {
                write!(f, "for {var} ∈ {iter} ")?;
                write_block(f, body)
            }
            PStmt::While { cond, body } => {
                write!(f, "while {cond} ")?;
                write_block(f, body)
            }
            PStmt::Break => write!(f, "break"),
            PStmt::Continue => write!(f, "continue"),
            PStmt::Return(e) => write!(f, "return {e}"),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Var(v) => write!(f, "{v}"),
            Target::Func(n, p) => write!(f, "{n}({})", p.join(", ")),
            Target::Indexed(n, idx) => {
                write!(f, "{n}[")?;
                write_list(f, idx)?;
                write!(f, "]")
            }
        }
    }
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Define { target, expr, global } => {
                write!(f, "{target} {} {expr}", if *global { "≡" } else { ":=" })
            }
            Stmt::Eval { expr, unit } => {
                write!(f, "{expr} =")?;
                if let Some(u) = unit {
                    write!(f, " {u}")?;
                }
                Ok(())
            }
            Stmt::Show(e) => write!(f, "{e}"),
        }
    }
}
