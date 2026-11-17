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
}

impl Expr {
    pub fn precedence(&self) -> u8 {
        match self {
            Expr::Range(..) => 0,
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
