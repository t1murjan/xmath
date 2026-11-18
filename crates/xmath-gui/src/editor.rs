//! Structural (Mathcad style, 2D) editing model of a math region.
//!
//! The edited statement is kept as an `Expr` tree with explicit placeholders
//! (`Var("■")`). Identifiers and numbers are `Var` tokens holding the typed text
//! (they only become numbers when the source is parsed again). Parentheses the
//! user typed are `Call("", [x])`, which prints as `(x)`.
//!
//! The cursor is a node path plus a position: inside a token it is a character
//! offset, on a placeholder it is 0, and on any other node 0 means "before" and
//! 1 "after" the node. The "selection" (the underlined operand Mathcad shows)
//! is the node under the cursor unless it was grown with Space.
//!
//! This module has no GUI dependencies; `structural.rs` draws it and feeds keys.

use std::collections::HashMap;
use xmath_core::ast::{BinOp, Expr, Stmt, Target, UnOp};

pub const PH: &str = "■";
const PH_IDENT: &str = "ph__";

/// Child indices from a slot of the statement down to a node (`path[0]` is the slot).
pub type Path = Vec<usize>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Form {
    /// A bare expression.
    Plain,
    /// `expr =`
    Eval,
    /// `lhs := rhs` (slots 0 and 1).
    Define { global: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    pub path: Path,
    pub pos: usize,
}

/// Keys of the structural editor that are not plain characters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum K {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    Tab,
    BackTab,
    Insert,
    /// Shift+Right: grow the selection (like Space).
    GrowRight,
    /// Shift+Left: shrink the selection back towards the cursor.
    ShrinkLeft,
}

#[derive(Clone, Debug)]
struct Snap {
    form: Form,
    slots: Vec<Expr>,
    show: bool,
    cur: Cursor,
    sel: Option<Path>,
}

#[derive(Clone, Debug)]
pub struct Ed {
    pub form: Form,
    /// `Define`: `[lhs, rhs]`; otherwise `[body]`; followed by the display unit slot after `=`.
    pub slots: Vec<Expr>,
    /// `Define` that also shows its value (`d := e =`).
    pub show: bool,
    pub cur: Cursor,
    /// Selection grown with Space (an ancestor of the cursor node).
    pub sel: Option<Path>,
    /// Mathcad's Insert key: operators go to the left of the operand.
    pub insert_left: bool,
    /// Set when a key cannot be expressed structurally: the source text to continue editing as a line.
    pub fallback: Option<String>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
}

// ---------------------------------------------------------------------------
// Tree helpers

pub fn ph() -> Expr {
    Expr::Var(PH.into())
}

pub fn is_ph(e: &Expr) -> bool {
    matches!(e, Expr::Var(n) if n == PH)
}

fn is_opaque(e: &Expr) -> bool {
    matches!(e, Expr::Matrix(_) | Expr::Program(_) | Expr::Str(_) | Expr::Num(_) | Expr::Imag(_))
}

pub fn kids(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) => vec![a],
        Expr::Binary(_, a, b) | Expr::OnError(a, b) | Expr::MatCol(a, b) => vec![a, b],
        Expr::Call(_, args) => args.iter().collect(),
        Expr::Index(b, idx) => std::iter::once(&**b).chain(idx.iter()).collect(),
        Expr::Range(a, b, c) => {
            let mut v = vec![&**a];
            if let Some(b) = b {
                v.push(b);
            }
            v.push(c);
            v
        }
        _ => vec![],
    }
}

pub fn kids_mut(e: &mut Expr) -> Vec<&mut Expr> {
    match e {
        Expr::Unary(_, a) | Expr::Abs(a) | Expr::Vectorize(a) => vec![a],
        Expr::Binary(_, a, b) | Expr::OnError(a, b) | Expr::MatCol(a, b) => vec![a, b],
        Expr::Call(_, args) => args.iter_mut().collect(),
        Expr::Index(b, idx) => std::iter::once(&mut **b).chain(idx.iter_mut()).collect(),
        Expr::Range(a, b, c) => {
            let mut v = vec![&mut **a];
            if let Some(b) = b {
                v.push(b);
            }
            v.push(c);
            v
        }
        _ => vec![],
    }
}

/// Nodes that have a closing edge: the cursor can stand "after" them.
fn has_after(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Call(..)
            | Expr::Abs(_)
            | Expr::Vectorize(_)
            | Expr::Index(..)
            | Expr::MatCol(..)
            | Expr::Unary(UnOp::Factorial, _)
            | Expr::Binary(BinOp::Div | BinOp::Pow, ..)
    )
}

/// Nodes that start with a symbol of their own (the cursor can stand "before" them).
fn has_before(e: &Expr) -> bool {
    matches!(e, Expr::Call(..) | Expr::Abs(_) | Expr::Vectorize(_))
}

fn stops_of(e: &Expr, path: &mut Path, out: &mut Vec<Cursor>) {
    let c = |path: &Path, pos| Cursor { path: path.clone(), pos };
    if let Expr::Var(n) = e {
        if n == PH {
            out.push(c(path, 0));
        } else {
            for off in 0..=n.chars().count() {
                out.push(c(path, off));
            }
        }
    } else if is_opaque(e) {
        out.push(c(path, 0));
        out.push(c(path, 1));
    } else {
        if has_before(e) {
            out.push(c(path, 0));
        }
        for (i, k) in kids(e).into_iter().enumerate() {
            path.push(i);
            stops_of(k, path, out);
            path.pop();
        }
        if has_after(e) {
            out.push(c(path, 1));
        }
    }
}

fn token_chars(n: &str) -> Vec<char> {
    n.chars().collect()
}

/// Characters of identifiers and numbers.
fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '∞' | '°' | '′')
}

fn is_fixed_template(name: &str) -> bool {
    matches!(name, "diff" | "integrate" | "sum" | "prod" | "nthroot" | "cross" | "if")
}

/// Names whose call is deleted together with the parentheses (they have no meaning without them).
fn is_plain_wrapper(name: &str) -> bool {
    matches!(name, "" | "sqrt" | "abs" | "exp" | "transpose")
}

fn is_target(e: &Expr) -> bool {
    let name_ok = |n: &str| n != PH && n.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
    match e {
        Expr::Var(n) => name_ok(n),
        Expr::Call(n, args) => name_ok(n) && args.iter().all(|a| matches!(a, Expr::Var(_))),
        Expr::Index(b, _) => matches!(&**b, Expr::Var(n) if name_ok(n)),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Conversion from / to source

fn decode_ph(e: &mut Expr) {
    if matches!(e, Expr::Var(n) if n == PH_IDENT) {
        *e = ph();
    }
    match e {
        Expr::Matrix(rows) => rows.iter_mut().flatten().for_each(decode_ph),
        _ => kids_mut(e).into_iter().for_each(decode_ph),
    }
}

fn num_text(x: f64) -> String {
    if x.is_infinite() {
        "∞".into()
    } else {
        format!("{x}")
    }
}

/// Numbers become tokens (so that "1." and "0.0" survive typing), `ph__` becomes `■`.
fn normalize(e: &mut Expr) {
    match e {
        Expr::Num(x) => {
            *e = if *x < 0.0 { Expr::Unary(UnOp::Neg, Box::new(Expr::Var(num_text(-*x)))) } else { Expr::Var(num_text(*x)) };
        }
        Expr::Imag(x) => *e = Expr::Var(format!("{}i", num_text(*x))),
        Expr::Var(n) if n == PH_IDENT => *e = ph(),
        Expr::Matrix(rows) => rows.iter_mut().flatten().for_each(decode_ph),
        _ => kids_mut(e).into_iter().for_each(normalize),
    }
}

fn has_program(e: &Expr) -> bool {
    match e {
        Expr::Program(_) => true,
        Expr::Matrix(rows) => rows.iter().flatten().any(has_program),
        _ => kids(e).into_iter().any(has_program),
    }
}

/// Pre-order addresses of the matrix literals (nested ones included), as walked by the
/// in-place cell editor.
fn matrix_addrs(e: &Expr, out: &mut Vec<usize>) {
    if let Expr::Matrix(rows) = e {
        out.push(e as *const Expr as usize);
        rows.iter().flatten().for_each(|c| matrix_addrs(c, out));
    } else {
        kids(e).into_iter().for_each(|k| matrix_addrs(k, out));
    }
}

fn addr_paths(e: &Expr, path: &mut Path, out: &mut HashMap<usize, Path>) {
    out.insert(e as *const Expr as usize, path.clone());
    for (i, k) in kids(e).into_iter().enumerate() {
        path.push(i);
        addr_paths(k, path, out);
        path.pop();
    }
}

impl Ed {
    /// An empty editor (a single placeholder).
    pub fn new() -> Ed {
        Ed {
            form: Form::Plain,
            slots: vec![ph()],
            show: false,
            cur: Cursor { path: vec![0], pos: 0 },
            sel: None,
            insert_left: false,
            fallback: None,
            undo: vec![],
            redo: vec![],
        }
    }

    /// Builds the editor for the source of a region. `None` when the statement
    /// cannot be edited structurally (does not parse, programs, vector targets).
    pub fn from_source(src: &str) -> Option<Ed> {
        if src.trim().is_empty() {
            return Some(Ed::new());
        }
        let st = xmath_core::parse_statement(&src.replace('■', PH_IDENT)).ok()?;
        let mut ed = Ed::new();
        match st {
            Stmt::Define { target, expr, global, show } => {
                let lhs = match target {
                    Target::Var(n) => Expr::Var(n),
                    Target::Func(n, params) => Expr::Call(n, params.into_iter().map(Expr::Var).collect()),
                    Target::Indexed(n, idx) => Expr::Index(Box::new(Expr::Var(n)), idx),
                    Target::Vector(_) => return None,
                };
                ed.form = Form::Define { global };
                ed.slots = vec![lhs, expr];
                if let Some(u) = show {
                    ed.show = true;
                    ed.slots.push(u.unwrap_or_else(ph));
                }
            }
            Stmt::Eval { expr, unit } => {
                ed.form = Form::Eval;
                ed.slots = vec![expr];
                ed.slots.push(unit.unwrap_or_else(ph));
            }
            Stmt::Show(e) => ed.slots = vec![e],
        }
        if ed.slots.iter().any(has_program) {
            return None;
        }
        ed.slots.iter_mut().for_each(normalize);
        let last = ed.slots.len() - 1;
        ed.cur = ed.end_of(&[last]);
        Some(ed)
    }

    /// The statement as parseable source (`■` for placeholders; empty for a fresh editor).
    pub fn source(&self) -> String {
        let unit = || self.unit_expr().filter(|u| !is_ph(u)).map(|u| format!(" {u}")).unwrap_or_default();
        match self.form {
            Form::Plain => {
                if is_ph(&self.slots[0]) {
                    String::new()
                } else {
                    self.slots[0].to_string()
                }
            }
            Form::Eval => format!("{} ={}", self.slots[0], unit()),
            Form::Define { global } => {
                let tail = if self.show { format!(" ={}", unit()) } else { String::new() };
                format!("{} {} {}{}", self.slots[0], if global { "≡" } else { ":=" }, self.slots[1], tail)
            }
        }
    }

    /// Number of slots before the unit slot.
    pub fn base_slots(&self) -> usize {
        if matches!(self.form, Form::Define { .. }) {
            2
        } else {
            1
        }
    }

    /// The display unit typed after `=` (a placeholder until something is typed).
    pub fn unit_expr(&self) -> Option<&Expr> {
        self.slots.get(self.base_slots())
    }

    pub fn has_placeholder(&self) -> bool {
        !self.stops().iter().all(|s| self.node(&s.path).is_none_or(|n| !is_ph(n)))
    }

    /// Addresses of the slot nodes' sub-expressions → paths (for the typesetter's hit boxes).
    pub fn addr_map(&self) -> HashMap<usize, Path> {
        let mut m = HashMap::new();
        for (i, s) in self.slots.iter().enumerate() {
            addr_paths(s, &mut vec![i], &mut m);
        }
        m
    }

    /// Addresses of matrix literals in the order the cell editor numbers them.
    pub fn matrix_order(&self) -> Vec<usize> {
        let mut v = Vec::new();
        self.slots.iter().for_each(|s| matrix_addrs(s, &mut v));
        v
    }

    // -----------------------------------------------------------------
    // Access

    pub fn node(&self, path: &[usize]) -> Option<&Expr> {
        let mut e = self.slots.get(*path.first()?)?;
        for &i in &path[1..] {
            e = *kids(e).get(i)?;
        }
        Some(e)
    }

    fn node_mut(&mut self, path: &[usize]) -> Option<&mut Expr> {
        let mut e = self.slots.get_mut(*path.first()?)?;
        for &i in &path[1..] {
            e = kids_mut(e).into_iter().nth(i)?;
        }
        Some(e)
    }

    fn replace(&mut self, path: &[usize], new: Expr) {
        if let Some(n) = self.node_mut(path) {
            *n = new;
        }
    }

    /// All cursor positions in visual (left to right) order.
    pub fn stops(&self) -> Vec<Cursor> {
        let mut out = Vec::new();
        for (i, s) in self.slots.iter().enumerate() {
            stops_of(s, &mut vec![i], &mut out);
        }
        out
    }

    fn subtree_stops(&self, path: &[usize]) -> Vec<Cursor> {
        let mut out = Vec::new();
        if let Some(n) = self.node(path) {
            stops_of(n, &mut path.to_vec(), &mut out);
        }
        out
    }

    fn end_of(&self, path: &[usize]) -> Cursor {
        self.subtree_stops(path).pop().unwrap_or(Cursor { path: path.to_vec(), pos: 0 })
    }

    /// First position inside the subtree (not the "before" of a wrapper).
    fn start_of(&self, path: &[usize]) -> Cursor {
        let stops = self.subtree_stops(path);
        stops
            .iter()
            .find(|s| self.node(&s.path).is_some_and(|n| matches!(n, Expr::Var(_))))
            .or(stops.first())
            .cloned()
            .unwrap_or(Cursor { path: path.to_vec(), pos: 0 })
    }

    /// The node an operator applies to: the grown selection or the node under the cursor.
    pub fn selected_path(&self) -> Path {
        self.sel.clone().unwrap_or_else(|| self.cur.path.clone())
    }

    fn snap(&mut self) {
        let s = Snap { form: self.form, slots: self.slots.clone(), show: self.show, cur: self.cur.clone(), sel: self.sel.clone() };
        self.undo.push(s);
        if self.undo.len() > 300 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, s: Snap) {
        self.form = s.form;
        self.slots = s.slots;
        self.show = s.show;
        self.cur = s.cur;
        self.sel = s.sel;
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else { return false };
        let now = Snap { form: self.form, slots: self.slots.clone(), show: self.show, cur: self.cur.clone(), sel: self.sel.clone() };
        self.redo.push(now);
        self.restore(prev);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        let now = Snap { form: self.form, slots: self.slots.clone(), show: self.show, cur: self.cur.clone(), sel: self.sel.clone() };
        self.undo.push(now);
        self.restore(next);
        true
    }

    /// Moves the cursor to a given stop (mouse clicks).
    pub fn set_cursor(&mut self, c: Cursor) {
        self.cur = c;
        self.sel = None;
        self.fix_cursor();
    }

    /// Makes the cursor valid after the tree changed.
    fn fix_cursor(&mut self) {
        let ok = match self.node(&self.cur.path) {
            Some(n) => {
                let max = match n {
                    Expr::Var(v) if v != PH => v.chars().count(),
                    Expr::Var(_) => 0,
                    _ => 1,
                };
                self.cur.pos <= max
            }
            None => false,
        };
        if !ok {
            let stops = self.stops();
            match self.node(&self.cur.path).map(|_| ()) {
                Some(()) => {
                    let max = match self.node(&self.cur.path) {
                        Some(Expr::Var(v)) if v != PH => v.chars().count(),
                        Some(Expr::Var(_)) => 0,
                        _ => 1,
                    };
                    self.cur.pos = self.cur.pos.min(max);
                }
                None => {
                    if let Some(l) = stops.last() {
                        self.cur = l.clone();
                    }
                }
            }
        }
        if let Some(s) = &self.sel {
            if self.node(s).is_none() {
                self.sel = None;
            }
        }
    }

    fn first_ph_in(&self, path: &[usize]) -> Option<Cursor> {
        self.subtree_stops(path).into_iter().find(|s| self.node(&s.path).is_some_and(is_ph))
    }

    // -----------------------------------------------------------------
    // Typing

    /// Types one character (Mathcad keystrokes included).
    pub fn type_char(&mut self, c: char) {
        self.fallback = None;
        match c {
            '■' | '\r' | '\n' | '\t' => {}
            ' ' => self.grow(),
            c if c.is_control() => {}
            c if is_token_char(c) => self.type_token(c),
            '+' => self.binop(BinOp::Add),
            '-' | '−' => self.binop(BinOp::Sub),
            '*' | '·' | '×' => self.binop(BinOp::Mul),
            '/' | '÷' => self.binop(BinOp::Div),
            '^' => self.binop(BinOp::Pow),
            '<' => self.binop(BinOp::Lt),
            '>' => self.binop(BinOp::Gt),
            '≤' => self.binop(BinOp::Le),
            '≥' => self.binop(BinOp::Ge),
            '≠' => self.binop(BinOp::Ne),
            '∧' => self.binop(BinOp::And),
            '∨' => self.binop(BinOp::Or),
            '¬' => self.prefix_not(),
            '(' => self.open_paren(),
            ')' => self.close(&[CloseKind::Paren]),
            '[' => self.subscript(),
            ']' => self.close(&[CloseKind::Index]),
            ',' => self.comma(),
            '|' => self.bar(),
            '\\' => self.template("sqrt(■)"),
            '?' => self.template("diff(■, ■)"),
            '&' => self.template("integrate(■, ■, ■, ■)"),
            '$' => self.template("sum(■, ■)"),
            '#' => self.template("prod(■, ■)"),
            ':' => self.colon(false),
            '~' | '≡' => self.colon(true),
            '=' => self.equals(),
            ';' => self.range(),
            '!' => self.postfix(|x| Expr::Unary(UnOp::Factorial, Box::new(x))),
            '\'' => self.wrap_paren(),
            _ => {
                // strings, programs and everything else: continue in the line editor
                let mut s = self.source();
                s.push(c);
                self.fallback = Some(s);
            }
        }
        self.fix_cursor();
    }

    pub fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            self.type_char(c);
        }
    }

    fn type_token(&mut self, c: char) {
        self.snap();
        if let Some(sel) = self.sel.take() {
            self.replace(&sel, Expr::Var(c.to_string()));
            self.cur = Cursor { path: sel, pos: 1 };
            return;
        }
        let path = self.cur.path.clone();
        let Some(node) = self.node(&path).cloned() else { return };
        match node {
            Expr::Var(n) if n != PH => {
                let mut ch = token_chars(&n);
                let p = self.cur.pos.min(ch.len());
                ch.insert(p, c);
                self.replace(&path, Expr::Var(ch.into_iter().collect()));
                self.cur.pos = p + 1;
            }
            Expr::Var(_) => {
                self.replace(&path, Expr::Var(c.to_string()));
                self.cur.pos = 1;
            }
            other => {
                if self.cur.pos >= 1 {
                    self.replace(&path, Expr::Binary(BinOp::Mul, Box::new(other), Box::new(Expr::Var(c.to_string()))));
                    let mut p = path;
                    p.push(1);
                    self.cur = Cursor { path: p, pos: 1 };
                } else {
                    self.replace(&path, Expr::Binary(BinOp::Mul, Box::new(Expr::Var(c.to_string())), Box::new(other)));
                    let mut p = path;
                    p.push(0);
                    self.cur = Cursor { path: p, pos: 1 };
                }
            }
        }
    }

    fn child_path(path: &[usize], i: usize) -> Path {
        let mut p = path.to_vec();
        p.push(i);
        p
    }

    fn on_ph(&mut self, path: &[usize]) {
        self.cur = Cursor { path: path.to_vec(), pos: 0 };
        self.sel = None;
    }

    /// Climbs from the operand while the operator binds weaker than the enclosing inline operator.
    fn climb(&self, mut path: Path, op: BinOp) -> Path {
        if matches!(op, BinOp::Div | BinOp::Pow) {
            return path;
        }
        while path.len() >= 2 {
            let last = path[path.len() - 1];
            let Some(p) = self.node(&path[..path.len() - 1]) else { break };
            let ok = match p {
                Expr::Binary(pop, ..) if !matches!(pop, BinOp::Div | BinOp::Pow) => last == 1 && pop.precedence() >= op.precedence(),
                Expr::Unary(UnOp::Neg | UnOp::Not, _) => p.precedence() >= op.precedence(),
                _ => false,
            };
            if !ok {
                break;
            }
            path.pop();
        }
        path
    }

    fn binop(&mut self, op: BinOp) {
        self.snap();
        let explicit = self.sel.take();
        let path = self.cur.path.clone();
        let Some(node) = self.node(&path).cloned() else { return };
        if let Some(sp) = explicit {
            let x = self.node(&sp).cloned().unwrap_or_else(ph);
            self.wrap_bin(&sp, op, x, self.insert_left);
            return;
        }
        if is_ph(&node) {
            if op == BinOp::Sub {
                self.replace(&path, Expr::Unary(UnOp::Neg, Box::new(ph())));
                let c = Self::child_path(&path, 0);
                self.on_ph(&c);
            } else if matches!(op, BinOp::Div | BinOp::Pow) {
                self.wrap_bin(&path, op, ph(), false);
            }
            return;
        }
        let mut left = self.insert_left;
        if let Expr::Var(n) = &node {
            let ch = token_chars(n);
            let p = self.cur.pos.min(ch.len());
            if p > 0 && p < ch.len() {
                // split the token: `ab` -> `a + b`
                let (l, r) = (ch[..p].iter().collect::<String>(), ch[p..].iter().collect::<String>());
                self.replace(&path, Expr::Binary(op, Box::new(Expr::Var(l)), Box::new(Expr::Var(r))));
                self.cur = Cursor { path: Self::child_path(&path, 1), pos: 0 };
                return;
            }
            if p == 0 {
                if op == BinOp::Sub {
                    self.replace(&path, Expr::Unary(UnOp::Neg, Box::new(node)));
                    self.cur = Cursor { path: Self::child_path(&path, 0), pos: 0 };
                    return;
                }
                left = true;
            }
        } else if self.cur.pos == 0 {
            left = true;
        }
        let target = if left { path } else { self.climb(path, op) };
        let x = self.node(&target).cloned().unwrap_or_else(ph);
        self.wrap_bin(&target, op, x, left);
    }

    fn wrap_bin(&mut self, path: &[usize], op: BinOp, x: Expr, left: bool) {
        let (new, c) = if left { (Expr::Binary(op, Box::new(ph()), Box::new(x)), 0) } else { (Expr::Binary(op, Box::new(x), Box::new(ph())), 1) };
        self.replace(path, new);
        let p = Self::child_path(path, c);
        self.on_ph(&p);
    }

    /// Public form of the typed operators (Ctrl+= etc.).
    pub fn type_binop(&mut self, op: BinOp) {
        self.binop(op);
        self.fix_cursor();
    }

    fn prefix_not(&mut self) {
        self.snap();
        let path = self.selected_path();
        let node = self.node(&path).cloned().unwrap_or_else(ph);
        if is_ph(&node) {
            self.replace(&path, Expr::Unary(UnOp::Not, Box::new(ph())));
            let c = Self::child_path(&path, 0);
            self.on_ph(&c);
        } else {
            self.replace(&path, Expr::Unary(UnOp::Not, Box::new(node)));
            self.cur = self.end_of(&path);
            self.sel = None;
        }
    }

    /// Postfix operator on the operand (`!`, `'`...): no climbing.
    fn postfix(&mut self, f: impl FnOnce(Expr) -> Expr) {
        self.snap();
        let path = self.selected_path();
        self.sel = None;
        let node = self.node(&path).cloned().unwrap_or_else(ph);
        if is_ph(&node) {
            return;
        }
        self.replace(&path, f(node));
        self.cur = self.end_of(&path);
    }

    fn wrap_paren(&mut self) {
        self.postfix(|x| Expr::Call(String::new(), vec![x]));
    }

    /// Operand with a bracket template: `Index`, `MatCol`.
    fn wrap_with_slot(&mut self, f: impl FnOnce(Expr) -> Expr, slot: usize) {
        self.snap();
        let path = self.selected_path();
        self.sel = None;
        let node = self.node(&path).cloned().unwrap_or_else(ph);
        if is_ph(&node) {
            return;
        }
        self.replace(&path, f(node));
        let p = Self::child_path(&path, slot);
        self.on_ph(&p);
    }

    fn subscript(&mut self) {
        self.wrap_with_slot(|x| Expr::Index(Box::new(x), vec![ph()]), 1);
    }

    /// `^<■>` (matrix column).
    pub fn matcol(&mut self) {
        self.wrap_with_slot(|x| Expr::MatCol(Box::new(x), Box::new(ph())), 1);
        self.fix_cursor();
    }

    fn open_paren(&mut self) {
        self.snap();
        if let Some(sel) = self.sel.take() {
            let x = self.node(&sel).cloned().unwrap_or_else(ph);
            self.replace(&sel, Expr::Call(String::new(), vec![x]));
            self.cur = self.end_of(&sel);
            return;
        }
        let path = self.cur.path.clone();
        let Some(node) = self.node(&path).cloned() else { return };
        if is_ph(&node) {
            self.replace(&path, Expr::Call(String::new(), vec![ph()]));
            let c = Self::child_path(&path, 0);
            self.on_ph(&c);
            return;
        }
        if let Expr::Var(n) = &node {
            let named = n.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
            if named && self.cur.pos >= n.chars().count() {
                self.replace(&path, Expr::Call(n.clone(), vec![ph()]));
                let c = Self::child_path(&path, 0);
                self.on_ph(&c);
                return;
            }
        }
        if self.cur.pos == 0 && !matches!(node, Expr::Var(_)) {
            self.replace(&path, Expr::Binary(BinOp::Mul, Box::new(Expr::Call(String::new(), vec![ph()])), Box::new(node)));
            let c = Self::child_path(&path, 0);
            self.on_ph(&Self::child_path(&c, 0));
            return;
        }
        self.replace(&path, Expr::Binary(BinOp::Mul, Box::new(node), Box::new(Expr::Call(String::new(), vec![ph()]))));
        let c = Self::child_path(&Self::child_path(&path, 1), 0);
        self.on_ph(&c);
    }

    /// `)` / `]`: leaves the nearest enclosing call, parentheses or subscript.
    fn close(&mut self, kinds: &[CloseKind]) {
        let mut path = self.cur.path.clone();
        while path.len() > 1 {
            let idx = path.pop().unwrap();
            let Some(p) = self.node(&path) else { return };
            let hit = match p {
                Expr::Call(..) => kinds.contains(&CloseKind::Paren),
                Expr::Index(..) => kinds.contains(&CloseKind::Index) && idx >= 1,
                _ => false,
            };
            if hit {
                self.cur = Cursor { path, pos: 1 };
                self.sel = None;
                return;
            }
        }
    }

    fn comma(&mut self) {
        let mut path = self.cur.path.clone();
        while path.len() > 1 {
            let idx = path.pop().unwrap();
            let Some(p) = self.node(&path).cloned() else { return };
            match p {
                Expr::Call(..) | Expr::Index(..) => {
                    // the index of the child in the argument list
                    let first_arg = usize::from(matches!(p, Expr::Index(..)));
                    if idx < first_arg {
                        continue;
                    }
                    self.snap();
                    let at = idx - first_arg + 1;
                    if let Some(n) = self.node_mut(&path) {
                        match n {
                            Expr::Call(_, args) => args.insert(at, ph()),
                            Expr::Index(_, args) => args.insert(at, ph()),
                            _ => {}
                        }
                    }
                    let c = Self::child_path(&path, idx + 1);
                    self.on_ph(&c);
                    return;
                }
                _ => {}
            }
        }
    }

    fn bar(&mut self) {
        // inside |...| at its end: leave it
        let mut path = self.cur.path.clone();
        while path.len() > 1 {
            path.pop();
            if matches!(self.node(&path), Some(Expr::Abs(_))) {
                let at_end = self.end_of(&Self::child_path(&path, 0)) == self.cur;
                if at_end {
                    self.cur = Cursor { path, pos: 1 };
                    self.sel = None;
                    return;
                }
                break;
            }
        }
        self.snap();
        let sp = self.selected_path();
        let node = self.node(&sp).cloned().unwrap_or_else(ph);
        self.sel = None;
        if is_ph(&node) {
            self.replace(&sp, Expr::Abs(Box::new(ph())));
            let c = Self::child_path(&sp, 0);
            self.on_ph(&c);
        } else {
            self.replace(&sp, Expr::Abs(Box::new(node)));
            self.cur = self.end_of(&sp);
        }
    }

    /// Inserts a template (source with `■`) at the cursor: replaces a placeholder or the
    /// selection, otherwise multiplies the operand by it.
    pub fn template(&mut self, src: &str) {
        let Ok(mut e) = xmath_core::parse_expr(&src.replace('■', PH_IDENT)) else { return };
        normalize(&mut e);
        self.insert_expr(e);
    }

    pub fn insert_expr(&mut self, e: Expr) {
        self.snap();
        let explicit = self.sel.take();
        let path = explicit.clone().unwrap_or_else(|| self.cur.path.clone());
        let Some(node) = self.node(&path).cloned() else { return };
        let target = if explicit.is_some() || is_ph(&node) {
            self.replace(&path, e);
            path.clone()
        } else if self.insert_left || (self.cur.pos == 0 && !matches!(node, Expr::Var(_))) {
            self.replace(&path, Expr::Binary(BinOp::Mul, Box::new(e), Box::new(node)));
            Self::child_path(&path, 0)
        } else {
            self.replace(&path, Expr::Binary(BinOp::Mul, Box::new(node), Box::new(e)));
            Self::child_path(&path, 1)
        };
        self.cur = self.first_ph_in(&target).unwrap_or_else(|| self.end_of(&target));
        self.fix_cursor();
    }

    /// Palette / paste insertion of source text. `false`: it needs the line editor.
    pub fn insert_str(&mut self, s: &str) -> bool {
        let t = s.trim();
        let needs_line = t.contains('{')
            || t.contains('←')
            || t.contains("on error")
            || t.starts_with("if ")
            || t.starts_with("for ")
            || t.starts_with("while ")
            || t.starts_with("return")
            || t == "break"
            || t == "continue"
            || t.starts_with(';')
            || t.contains('"');
        if needs_line {
            return false;
        }
        match t {
            ":=" => self.type_char(':'),
            "≡" => self.type_char('~'),
            "==" => self.type_binop(BinOp::Eq),
            ".." => self.type_char(';'),
            "(■)" => self.type_char('('),
            "[■]" => self.type_char('['),
            "^<■>" => self.matcol(),
            _ => {
                let starts_ph = t.starts_with('■');
                let first = t.chars().next();
                let looks_expr = first.is_some_and(|c| c.is_alphanumeric() || matches!(c, '(' | '[' | '|' | '-'))
                    && (t.contains('■') || t.contains('('))
                    && !starts_ph;
                if looks_expr {
                    if let Ok(mut e) = xmath_core::parse_expr(&t.replace('■', PH_IDENT)) {
                        normalize(&mut e);
                        self.insert_expr(e);
                        return true;
                    }
                }
                self.type_str(t);
            }
        }
        true
    }

    fn colon(&mut self, global: bool) {
        match self.form {
            Form::Plain if self.cur.path[0] == 0 => {
                if !is_target(&self.slots[0]) {
                    return;
                }
                self.snap();
                let lhs = self.slots.remove(0);
                self.slots = vec![lhs, ph()];
                self.form = Form::Define { global };
                self.cur = Cursor { path: vec![1], pos: 0 };
                self.sel = None;
            }
            Form::Define { .. } if self.cur.path[0] == 0 => {
                self.snap();
                self.form = Form::Define { global };
                self.cur = self.start_of(&[1]);
                self.sel = None;
            }
            _ => {}
        }
    }

    fn equals(&mut self) {
        match self.form {
            Form::Plain => {
                if is_ph(&self.slots[0]) {
                    return;
                }
                self.snap();
                self.form = Form::Eval;
                self.slots.push(ph());
                self.cur = Cursor { path: vec![1], pos: 0 };
                self.sel = None;
            }
            Form::Define { .. } if !self.show && self.cur.path[0] == 1 => {
                self.snap();
                self.show = true;
                self.slots.push(ph());
                self.cur = Cursor { path: vec![2], pos: 0 };
                self.sel = None;
            }
            _ => {}
        }
    }

    /// `;` -> `a..b`: the range applies to the whole expression of the slot (or call argument).
    fn range(&mut self) {
        self.snap();
        let mut path = self.selected_path();
        self.sel = None;
        while path.len() >= 2 {
            let last = path[path.len() - 1];
            let Some(p) = self.node(&path[..path.len() - 1]) else { break };
            let ok = match p {
                Expr::Binary(op, ..) if !matches!(op, BinOp::Div | BinOp::Pow) => last == 1,
                Expr::Unary(UnOp::Neg | UnOp::Not, _) => true,
                _ => false,
            };
            if !ok {
                break;
            }
            path.pop();
        }
        let x = self.node(&path).cloned().unwrap_or_else(ph);
        if is_ph(&x) {
            return;
        }
        self.replace(&path, Expr::Range(Box::new(x), None, Box::new(ph())));
        let c = Self::child_path(&path, 1);
        self.on_ph(&c);
    }

    // -----------------------------------------------------------------
    // Selection

    /// Space: grows the selection to the enclosing operand.
    pub fn grow(&mut self) {
        let cur = self.selected_path();
        if cur.len() > 1 {
            let mut p = cur;
            p.pop();
            self.sel = Some(p);
        }
    }

    /// Shift+Left: shrinks the grown selection back towards the cursor.
    pub fn shrink(&mut self) {
        if let Some(sel) = self.sel.clone() {
            let cp = &self.cur.path;
            if cp.len() > sel.len() && cp[..sel.len()] == sel[..] {
                let p = cp[..sel.len() + 1].to_vec();
                self.sel = if p == *cp { None } else { Some(p) };
            } else {
                self.sel = None;
            }
        }
    }

    // -----------------------------------------------------------------
    // Keys

    pub fn press(&mut self, k: K) {
        self.fallback = None;
        match k {
            K::Left => self.step(-1),
            K::Right => self.step(1),
            K::Up => self.vertical(true),
            K::Down => self.vertical(false),
            K::Home => {
                if let Some(s) = self.stops().first() {
                    self.set_cursor(s.clone());
                }
            }
            K::End => {
                if let Some(s) = self.stops().last() {
                    self.set_cursor(s.clone());
                }
            }
            K::Tab => self.tab(false),
            K::BackTab => self.tab(true),
            K::Insert => self.insert_left = !self.insert_left,
            K::GrowRight => self.grow(),
            K::ShrinkLeft => self.shrink(),
            K::Backspace => {
                self.snap();
                self.backspace();
            }
            K::Delete => {
                self.snap();
                self.delete();
            }
        }
        self.fix_cursor();
    }

    fn index_of_cursor(&self, stops: &[Cursor]) -> Option<usize> {
        stops.iter().position(|s| *s == self.cur)
    }

    fn step(&mut self, d: i32) {
        let stops = self.stops();
        let Some(i) = self.index_of_cursor(&stops) else {
            if let Some(l) = stops.last() {
                self.cur = l.clone();
            }
            return;
        };
        let j = i as i32 + d;
        if j >= 0 && (j as usize) < stops.len() {
            self.cur = stops[j as usize].clone();
        }
        self.sel = None;
    }

    fn vertical(&mut self, up: bool) {
        let mut path = self.cur.path.clone();
        while path.len() > 1 {
            let j = path.pop().unwrap();
            let Some(a) = self.node(&path) else { return };
            let target = match (a, up, j) {
                (Expr::Binary(BinOp::Div, ..), true, 1) => Some(self.end_of(&Self::child_path(&path, 0))),
                (Expr::Binary(BinOp::Div, ..), false, 0) => Some(self.start_of(&Self::child_path(&path, 1))),
                (Expr::Binary(BinOp::Pow, ..), true, 0) => Some(self.start_of(&Self::child_path(&path, 1))),
                (Expr::Binary(BinOp::Pow, ..), false, 1) => Some(self.end_of(&Self::child_path(&path, 0))),
                (Expr::Index(..), false, 0) => Some(self.start_of(&Self::child_path(&path, 1))),
                (Expr::Index(..), true, j) if j >= 1 => Some(self.end_of(&Self::child_path(&path, 0))),
                _ => None,
            };
            if let Some(t) = target {
                self.cur = t;
                self.sel = None;
                return;
            }
        }
    }

    fn is_slotted(e: &Expr) -> bool {
        matches!(e, Expr::Call(..) | Expr::Index(..) | Expr::Range(..) | Expr::MatCol(..) | Expr::OnError(..) | Expr::Binary(BinOp::Div | BinOp::Pow, ..))
    }

    /// Tab / Shift+Tab: to the next / previous operand slot of the enclosing structure
    /// (call argument, fraction part, subscript...); wraps around at the ends.
    fn tab(&mut self, back: bool) {
        let mut path = self.cur.path.clone();
        while path.len() > 1 {
            let j = path.pop().unwrap();
            let Some(a) = self.node(&path) else { return };
            if Self::is_slotted(a) {
                let n = kids(a).len();
                let to = if back { j.checked_sub(1) } else { (j + 1 < n).then_some(j + 1) };
                if let Some(to) = to {
                    self.cur = self.end_of(&Self::child_path(&path, to));
                    self.sel = None;
                    return;
                }
            }
        }
        // no further slot: jump to the first / last placeholder of the statement
        let stops = self.stops();
        let phs: Vec<&Cursor> = stops.iter().filter(|s| self.node(&s.path).is_some_and(is_ph)).collect();
        if let Some(c) = if back { phs.last() } else { phs.first() } {
            self.cur = (*c).clone();
            self.sel = None;
        }
    }

    fn selection_delete(&mut self, sel: Path) {
        if sel.len() == 1 {
            // a whole slot: clear it
            self.replace(&sel, ph());
        } else {
            self.replace(&sel, ph());
        }
        self.on_ph(&sel);
    }

    fn backspace(&mut self) {
        if let Some(sel) = self.sel.take() {
            self.selection_delete(sel);
            return;
        }
        let path = self.cur.path.clone();
        let Some(node) = self.node(&path).cloned() else { return };
        match &node {
            Expr::Var(n) if n != PH => {
                let mut ch = token_chars(n);
                let p = self.cur.pos.min(ch.len());
                if p > 0 {
                    ch.remove(p - 1);
                    if ch.is_empty() {
                        self.replace(&path, ph());
                        self.on_ph(&path);
                    } else {
                        self.replace(&path, Expr::Var(ch.into_iter().collect()));
                        self.cur.pos = p - 1;
                    }
                } else {
                    self.backspace_at_start(&path);
                }
            }
            Expr::Var(_) => self.delete_placeholder(&path),
            other => {
                if self.cur.pos >= 1 {
                    if is_opaque(other) {
                        self.replace(&path, ph());
                        self.on_ph(&path);
                    } else {
                        // step into the node from its closing edge
                        let mut s = self.subtree_stops(&path);
                        s.pop();
                        if let Some(l) = s.pop() {
                            self.cur = l;
                        }
                    }
                } else {
                    self.backspace_at_start(&path);
                }
            }
        }
    }

    /// Backspace with the cursor at the very start of the node at `path`.
    fn backspace_at_start(&mut self, path: &[usize]) {
        if path.len() == 1 {
            if path[0] >= self.base_slots() {
                self.cur = self.end_of(&[path[0] - 1]);
                return;
            }
            if let Form::Define { .. } = self.form {
                if path[0] == 1 {
                    if is_ph(&self.slots[1]) {
                        self.form = Form::Plain;
                        self.show = false;
                        self.slots.truncate(1);
                        self.cur = self.end_of(&[0]);
                    } else {
                        self.cur = self.end_of(&[0]);
                    }
                }
            }
            return;
        }
        let parent: Path = path[..path.len() - 1].to_vec();
        let idx = path[path.len() - 1];
        let Some(p) = self.node(&parent).cloned() else { return };
        match p {
            Expr::Binary(op, l, r) if idx == 1 => {
                let mergeable = !matches!(op, BinOp::Pow);
                match (&*l, &*r) {
                    _ if is_ph(&l) => {
                        self.replace(&parent, *r);
                        self.cur = self.start_of(&parent);
                    }
                    (Expr::Var(a), Expr::Var(b)) if mergeable => {
                        let n = a.chars().count();
                        self.replace(&parent, Expr::Var(format!("{a}{b}")));
                        self.cur = Cursor { path: parent, pos: n };
                    }
                    _ => self.cur = self.end_of(&Self::child_path(&parent, 0)),
                }
            }
            Expr::Unary(UnOp::Neg | UnOp::Not, a) => {
                self.replace(&parent, *a);
                self.cur = self.start_of(&parent);
            }
            Expr::Call(name, args) if idx == 0 => {
                let a = args[0].clone();
                if args.len() == 1 && is_plain_wrapper(&name) {
                    self.replace(&parent, a);
                    self.cur = self.start_of(&parent);
                } else if args.len() == 1 {
                    match &a {
                        Expr::Var(t) if t != PH => {
                            let n = name.chars().count();
                            self.replace(&parent, Expr::Var(format!("{name}{t}")));
                            self.cur = Cursor { path: parent, pos: n };
                        }
                        Expr::Var(_) => {
                            let n = name.chars().count();
                            self.replace(&parent, Expr::Var(name));
                            self.cur = Cursor { path: parent, pos: n };
                        }
                        _ => self.step(-1),
                    }
                } else {
                    self.step(-1)
                }
            }
            Expr::Call(_, args) if idx > 0 => {
                let prev = Self::child_path(&parent, idx - 1);
                if is_ph(&args[idx - 1]) {
                    if let Some(Expr::Call(_, a)) = self.node_mut(&parent) {
                        a.remove(idx - 1);
                    }
                    self.cur = self.start_of(&Self::child_path(&parent, idx - 1));
                } else {
                    self.cur = self.end_of(&prev);
                }
            }
            Expr::Abs(a) => {
                self.replace(&parent, *a);
                self.cur = self.start_of(&parent);
            }
            Expr::Index(b, idxs) if idx >= 1 => {
                if idxs.len() == 1 && idxs.iter().all(is_ph) {
                    self.replace(&parent, *b);
                    self.cur = self.end_of(&parent);
                } else {
                    self.cur = self.end_of(&Self::child_path(&parent, 0));
                }
            }
            _ => self.step(-1),
        }
    }

    /// Backspace / Delete on a placeholder: removes the empty structure around it.
    fn delete_placeholder(&mut self, path: &[usize]) {
        if path.len() == 1 {
            if path[0] >= self.base_slots() {
                // an empty unit: removes the `=`
                self.slots.truncate(self.base_slots());
                self.show = false;
                if self.form == Form::Eval {
                    self.form = Form::Plain;
                }
                self.cur = self.end_of(&[self.base_slots() - 1]);
                return;
            }
            if let Form::Define { .. } = self.form {
                if path[0] == 1 {
                    self.form = Form::Plain;
                    self.show = false;
                    self.slots.truncate(1);
                    self.cur = self.end_of(&[0]);
                } else {
                    let rhs = self.slots.remove(1);
                    self.slots = vec![rhs];
                    self.form = Form::Plain;
                    self.show = false;
                    self.cur = self.start_of(&[0]);
                }
            } else if self.form == Form::Eval {
                self.form = Form::Plain;
                self.slots.truncate(1);
            }
            return;
        }
        let parent: Path = path[..path.len() - 1].to_vec();
        let idx = path[path.len() - 1];
        let Some(p) = self.node(&parent).cloned() else { return };
        match p {
            Expr::Binary(_, l, r) | Expr::OnError(l, r) | Expr::MatCol(l, r) => {
                if idx == 1 {
                    self.replace(&parent, *l);
                    self.cur = self.end_of(&parent);
                } else {
                    self.replace(&parent, *r);
                    self.cur = self.start_of(&parent);
                }
            }
            Expr::Unary(..) | Expr::Abs(_) | Expr::Vectorize(_) => {
                self.replace(&parent, ph());
                self.on_ph(&parent);
            }
            Expr::Call(name, args) => {
                if args.len() == 1 || args.iter().all(is_ph) {
                    if is_plain_wrapper(&name) && name.is_empty() || name == "sqrt" || name == "abs" || name.is_empty() {
                        self.replace(&parent, ph());
                        self.on_ph(&parent);
                    } else {
                        let n = name.chars().count();
                        self.replace(&parent, Expr::Var(name));
                        self.cur = Cursor { path: parent, pos: n };
                    }
                } else if is_fixed_template(&name) {
                    self.step(-1);
                } else {
                    if let Some(Expr::Call(_, a)) = self.node_mut(&parent) {
                        a.remove(idx);
                    }
                    let at = idx.saturating_sub(1);
                    self.cur = if idx > 0 { self.end_of(&Self::child_path(&parent, at)) } else { self.start_of(&Self::child_path(&parent, 0)) };
                }
            }
            Expr::Index(b, idxs) => {
                if idx >= 1 {
                    if idxs.len() == 1 {
                        self.replace(&parent, *b);
                        self.cur = self.end_of(&parent);
                    } else {
                        if let Some(Expr::Index(_, a)) = self.node_mut(&parent) {
                            a.remove(idx - 1);
                        }
                        self.cur = self.end_of(&Self::child_path(&parent, idx - 1));
                    }
                } else if idxs.iter().all(is_ph) {
                    self.replace(&parent, ph());
                    self.on_ph(&parent);
                } else {
                    self.step(-1);
                }
            }
            Expr::Range(a, b, c) => {
                if idx == 0 {
                    self.replace(&parent, b.map(|b| *b).unwrap_or(*c));
                    self.cur = self.start_of(&parent);
                } else {
                    self.replace(&parent, *a);
                    self.cur = self.end_of(&parent);
                }
            }
            _ => self.step(-1),
        }
    }

    fn delete(&mut self) {
        if let Some(sel) = self.sel.take() {
            self.selection_delete(sel);
            return;
        }
        let path = self.cur.path.clone();
        let Some(node) = self.node(&path).cloned() else { return };
        match &node {
            Expr::Var(n) if n != PH => {
                let mut ch = token_chars(n);
                let p = self.cur.pos.min(ch.len());
                if p < ch.len() {
                    ch.remove(p);
                    if ch.is_empty() {
                        self.replace(&path, ph());
                        self.on_ph(&path);
                    } else {
                        self.replace(&path, Expr::Var(ch.into_iter().collect()));
                    }
                } else if path.len() >= 2 {
                    // join with the right operand of an inline operator
                    let parent: Path = path[..path.len() - 1].to_vec();
                    let idx = path[path.len() - 1];
                    if let Some(Expr::Binary(op, l, r)) = self.node(&parent).cloned() {
                        if idx == 0 && !matches!(op, BinOp::Pow) {
                            match (&*l, &*r) {
                                (_, r2) if is_ph(r2) => {
                                    self.replace(&parent, *l);
                                    self.cur = self.end_of(&parent);
                                }
                                (Expr::Var(a), Expr::Var(b)) => {
                                    let n = a.chars().count();
                                    self.replace(&parent, Expr::Var(format!("{a}{b}")));
                                    self.cur = Cursor { path: parent, pos: n };
                                }
                                _ => self.step(1),
                            }
                            return;
                        }
                    }
                    self.step(1);
                } else {
                    self.step(1);
                }
            }
            Expr::Var(_) => self.delete_placeholder(&path),
            other => {
                if self.cur.pos == 0 || is_opaque(other) {
                    self.replace(&path, ph());
                    self.on_ph(&path);
                } else {
                    self.step(1);
                }
            }
        }
    }

    /// Ctrl+G: the letter before the cursor becomes Greek (and back).
    pub fn greek(&mut self, f: impl Fn(char) -> Option<char>) {
        let path = self.cur.path.clone();
        let Some(Expr::Var(n)) = self.node(&path).cloned() else { return };
        let mut ch = token_chars(&n);
        let p = self.cur.pos.min(ch.len());
        if n == PH || p == 0 {
            return;
        }
        if let Some(g) = f(ch[p - 1]) {
            self.snap();
            ch[p - 1] = g;
            self.replace(&path, Expr::Var(ch.into_iter().collect()));
        }
    }

    /// Source text of the selected operand (Ctrl+C).
    pub fn copy_text(&self) -> String {
        self.node(&self.selected_path()).map(|n| n.to_string()).unwrap_or_default()
    }

    /// Ctrl+X: copy, then delete the selection.
    pub fn cut_text(&mut self) -> String {
        let t = self.copy_text();
        if self.sel.is_none() {
            self.sel = Some(self.cur.path.clone());
        }
        self.snap();
        if let Some(sel) = self.sel.take() {
            self.selection_delete(sel);
        }
        t
    }

    /// Ctrl+V: a parsable expression is inserted as a structure, anything else is typed.
    pub fn paste_text(&mut self, t: &str) {
        let t = t.trim();
        if t.is_empty() {
            return;
        }
        if let Ok(mut e) = xmath_core::parse_expr(&t.replace('■', PH_IDENT)) {
            normalize(&mut e);
            self.insert_expr(e);
        } else {
            self.type_str(t);
        }
    }
}

impl Default for Ed {
    fn default() -> Self {
        Ed::new()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CloseKind {
    Paren,
    Index,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(keys: &str) -> Ed {
        let mut e = Ed::new();
        e.type_str(keys);
        e
    }

    fn src(keys: &str) -> String {
        t(keys).source()
    }

    #[test]
    fn precedence_like_mathcad() {
        assert_eq!(src("a+b/c"), "a + b/c");
        assert_eq!(src("a*b+c"), "a*b + c");
        assert_eq!(src("a+b*c"), "a + b*c");
        assert_eq!(src("a-b+c"), "a - b + c");
        // an operator after a fraction slot stays inside the slot until the cursor leaves it
        let mut e = t("a/b+c");
        assert_eq!(e.source(), "a/(b + c)");
        e.press(K::Right);
        e.type_str("+d");
        assert_eq!(e.source(), "a/(b + c) + d");
    }

    #[test]
    fn space_grows_selection() {
        // select `a+b` with Space, then `/c`
        let mut e = t("a+b");
        e.type_char(' ');
        e.type_str("/c");
        assert_eq!(e.source(), "(a + b)/c");
        // Space twice with nothing above stays at the slot root
        let mut e = t("a+b*c");
        e.type_char(' ');
        e.type_char(' ');
        e.type_char(' ');
        e.type_str("^2");
        assert_eq!(e.source(), "(a + b*c)^2");
    }

    #[test]
    fn powers_and_exit() {
        assert_eq!(src("x^2+1"), "x^(2 + 1)");
        let mut e = t("x^2");
        e.press(K::Right);
        e.type_str("+1");
        assert_eq!(e.source(), "x^2 + 1");
        assert_eq!(src("x^-1"), "x^(-1)");
    }

    #[test]
    fn sqrt_backslash() {
        assert_eq!(src("\\x+1"), "sqrt(x + 1)");
        let mut e = t("\\x+1");
        e.press(K::Right);
        e.type_str("*2");
        assert_eq!(e.source(), "sqrt(x + 1)*2");
        assert_eq!(src("2*\\x"), "2*sqrt(x)");
    }

    #[test]
    fn function_calls() {
        assert_eq!(src("sin(x)+1"), "sin(x) + 1");
        assert_eq!(src("sin(x"), "sin(x)");
        assert_eq!(src("atan2(y,x)"), "atan2(y, x)");
        assert_eq!(src("f(x):x^2"), "f(x) := x^2");
        assert_eq!(src("sin("), "sin(■)");
    }

    #[test]
    fn definitions_and_eval() {
        assert_eq!(src("x:5"), "x := 5");
        assert_eq!(src("x:5="), "x := 5 =");
        assert_eq!(src("2+3="), "2 + 3 =");
        assert_eq!(src("g~9.81"), "g ≡ 9.81");
        assert_eq!(src("f(x,y):x*y"), "f(x, y) := x*y");
        // `:` after something that is no target is ignored
        assert_eq!(src("2+3:"), "2 + 3");
    }

    #[test]
    fn parens_and_abs() {
        assert_eq!(src("(a+b)*c"), "(a + b)*c");
        assert_eq!(src("2*(a+b)"), "2*(a + b)");
        assert_eq!(src("|x-1|+2"), "|x - 1| + 2");
        let mut e = t("a+b");
        e.type_char(' ');
        e.type_char('\'');
        assert_eq!(e.source(), "(a + b)");
    }

    #[test]
    fn subscripts_and_factorial() {
        assert_eq!(src("v[i+1]"), "v[i + 1]");
        assert_eq!(src("n!"), "n!");
        assert_eq!(src("v[i]="), "v[i] =");
    }

    #[test]
    fn range_and_text_fallback() {
        assert_eq!(src("i:1;10"), "i := 1..10");
        let e = t("a\"");
        assert_eq!(e.fallback.as_deref(), Some("a\""));
    }

    #[test]
    fn placeholders_show() {
        assert_eq!(src("a+"), "a + ■");
        assert_eq!(src("a/"), "a/■");
        assert_eq!(src("-"), "-■");
        assert_eq!(src("a+-"), "a + -■");
        assert!(t("a+").has_placeholder());
        assert!(!t("a+b").has_placeholder());
        assert_eq!(src(""), "");
    }

    #[test]
    fn templates() {
        assert_eq!(src("?x"), "diff(x, ■)");
        let mut e = Ed::new();
        e.insert_str("diff(■, ■)");
        e.type_str("x^2");
        e.press(K::Tab);
        e.type_str("x");
        assert_eq!(e.source(), "diff(x^2, x)");
        let mut e = t("x");
        assert!(e.insert_str("■!"));
        assert_eq!(e.source(), "x!");
        let mut e = t("x");
        assert!(e.insert_str("^2"));
        assert_eq!(e.source(), "x^2");
        let mut e = Ed::new();
        e.insert_str("■/■");
        assert_eq!(e.source(), "■/■");
        let mut e = t("a");
        e.insert_str("■/■");
        e.type_str("b");
        assert_eq!(e.source(), "a/b");
        assert!(!Ed::new().insert_str("{ ■ }"));
    }

    #[test]
    fn backspace_cases() {
        // characters, then the operator joins operands
        let mut e = t("ab");
        e.press(K::Backspace);
        assert_eq!(e.source(), "a");
        e.press(K::Backspace);
        assert_eq!(e.source(), "");
        let mut e = t("a+b");
        e.press(K::Left);
        e.press(K::Backspace); // deletes the operator
        assert_eq!(e.source(), "ab");
        // typing an operator and deleting it again
        let mut e = t("a+");
        e.press(K::Backspace);
        assert_eq!(e.source(), "a");
        e.type_str("+b");
        assert_eq!(e.source(), "a + b");
        // an empty denominator removes the fraction
        let mut e = t("a/");
        e.press(K::Backspace);
        assert_eq!(e.source(), "a");
        // function call: the paren pair goes, the name stays
        let mut e = t("sin(");
        e.press(K::Backspace);
        assert_eq!(e.source(), "sin");
        // sqrt with a placeholder disappears
        let mut e = t("2*\\");
        e.press(K::Backspace);
        assert_eq!(e.source(), "2*■");
        // `:=` with an empty right side
        let mut e = t("x:");
        e.press(K::Backspace);
        assert_eq!(e.source(), "x");
        // deleting a grown selection leaves a placeholder
        let mut e = t("a+b");
        e.type_char(' ');
        e.press(K::Backspace);
        assert_eq!(e.source(), "");
        let mut e = t("c*(a+b)");
        e.press(K::Left);
        e.press(K::Left);
        assert!(e.source().contains("a"));
    }

    #[test]
    fn backspace_into_structure() {
        // Backspace right after a closed call steps back into it
        let mut e = t("sin(x)");
        e.press(K::Backspace);
        e.press(K::Backspace);
        assert_eq!(e.source(), "sin(■)");
        // unwrapping at the start of the argument keeps the content
        let mut e = t("\\x");
        e.press(K::Left);
        e.press(K::Backspace);
        assert_eq!(e.source(), "x");
    }

    #[test]
    fn delete_key() {
        let mut e = t("ab");
        e.press(K::Home);
        e.press(K::Delete);
        assert_eq!(e.source(), "b");
        let mut e = t("a+b");
        e.press(K::Home);
        e.press(K::Right);
        e.press(K::Delete); // the operator after a
        assert_eq!(e.source(), "ab");
    }

    #[test]
    fn arrows_walk_through_slots() {
        let mut e = t("a/b");
        e.press(K::Up);
        e.type_str("+c");
        assert_eq!(e.source(), "(a + c)/b");
        e.press(K::Down);
        e.type_str("d");
        assert_eq!(e.source(), "(a + c)/db");
        // Left/Right pass the slots in order
        let mut e = t("x^2");
        e.press(K::Left);
        e.press(K::Left);
        e.type_str("y");
        assert_eq!(e.source(), "xy^2");
    }

    #[test]
    fn tab_between_placeholders() {
        let mut e = Ed::new();
        e.insert_str("integrate(■, ■, ■, ■)");
        e.type_str("f");
        e.press(K::Tab);
        e.type_str("x");
        e.press(K::Tab);
        e.type_str("0");
        e.press(K::Tab);
        e.type_str("1");
        assert_eq!(e.source(), "integrate(f, x, 0, 1)");
        e.press(K::BackTab);
        e.type_str("2");
        assert_eq!(e.source(), "integrate(f, x, 02, 1)");
    }

    #[test]
    fn comma_and_close() {
        let mut e = t("max(a,b)");
        assert_eq!(e.source(), "max(a, b)");
        e.type_str("+1");
        assert_eq!(e.source(), "max(a, b) + 1");
        // a closing parenthesis outside any call is ignored
        let e2 = t("a)");
        assert_eq!(e2.source(), "a");
    }

    #[test]
    fn undo_redo() {
        let mut e = t("a+b");
        assert!(e.undo());
        assert_eq!(e.source(), "a + ■");
        assert!(e.undo());
        assert_eq!(e.source(), "a");
        assert!(e.redo());
        assert_eq!(e.source(), "a + ■");
    }

    #[test]
    fn insert_left_mode() {
        let mut e = t("b");
        e.press(K::Insert);
        e.type_str("+");
        e.type_str("a");
        assert_eq!(e.source(), "a + b");
    }

    #[test]
    fn copy_paste() {
        let mut e = t("a+b");
        e.type_char(' ');
        assert_eq!(e.copy_text(), "a + b");
        let mut e2 = Ed::new();
        e2.paste_text("x^2 + 1");
        assert_eq!(e2.source(), "x^2 + 1");
        e2.type_str("*3");
        assert_eq!(e2.source(), "x^2 + 1*3");
    }

    #[test]
    fn load_roundtrip() {
        for s in ["f(x) := x^2 + 1", "y := sin(x)/2 =", "a + b*c =", "A := [1, 2; 3, 4]", "v[i] := i^2", "x ≡ 5", "d := a/b = μm"] {
            let e = Ed::from_source(s).unwrap_or_else(|| panic!("{s}"));
            let out = e.source();
            let a = xmath_core::parse_statement(s).unwrap();
            let b = xmath_core::parse_statement(&out).unwrap();
            assert_eq!(a, b, "{s} -> {out}");
        }
        assert!(Ed::from_source("{ s ← 0; s }").is_none());
        assert!(Ed::from_source("x := ").is_none());
        let e = Ed::from_source("x := ■ + 1").unwrap();
        assert!(e.has_placeholder());
        assert_eq!(e.source(), "x := ■ + 1");
    }

    #[test]
    fn edit_loaded_statement() {
        let mut e = Ed::from_source("f(x) := x^2").unwrap();
        e.press(K::Right);
        e.type_str("+1");
        assert_eq!(e.source(), "f(x) := x^2 + 1");
        e.type_char('=');
        assert_eq!(e.source(), "f(x) := x^2 + 1 =");
    }

    #[test]
    fn greek_letters() {
        let mut e = t("a");
        e.greek(|c| (c == 'a').then_some('α'));
        assert_eq!(e.source(), "α");
    }

    #[test]
    fn split_token_with_operator() {
        let mut e = t("ab");
        e.press(K::Left);
        e.type_char('+');
        assert_eq!(e.source(), "a + b");
    }

    #[test]
    fn units_after_equals() {
        assert_eq!(src("F=kN"), "F = kN");
        assert_eq!(src("d:a/b=m*s"), "d := a/b = m*s");
        // an empty unit slot: Backspace removes the `=`
        let mut e = t("x:5=");
        e.press(K::Backspace);
        assert_eq!(e.source(), "x := 5");
        let mut e = Ed::from_source("F = kN").unwrap();
        assert_eq!(e.source(), "F = kN");
        e.press(K::Backspace);
        e.press(K::Backspace);
        assert_eq!(e.source(), "F =");
        e.press(K::Backspace);
        assert_eq!(e.source(), "F");
    }

    #[test]
    fn comparison_operators() {
        assert_eq!(src("a≤b"), "a <= b");
        assert_eq!(src("a<b+c"), "a < b + c");
        let mut e = t("a");
        e.type_binop(BinOp::Eq);
        e.type_str("b");
        assert_eq!(e.source(), "a == b");
    }

    /// Evaluates `a := 3` followed by the typed region; returns its error text, if any.
    fn eval_error(keys: &str) -> (String, Option<String>) {
        let e = t(keys);
        let source = e.source();
        let mut ws = xmath_core::Worksheet::default();
        ws.regions.push(xmath_core::Region::new(1, 0.0, 0.0, xmath_core::RegionKind::Math { source: "a := 3".into() }));
        ws.regions.push(xmath_core::Region::new(2, 0.0, 100.0, xmath_core::RegionKind::Math { source: source.clone() }));
        let out = xmath_core::evaluate_worksheet(&ws);
        (source, out.get(&2).and_then(|o| o.error()))
    }

    #[test]
    fn results_after_equals_evaluate() {
        for keys in [
            "\\a^2+16=",
            "a*2=",
            "\\a+1=",
            "nthroot(a+5,3)=",
            "|a-7|=",
            "|a-7|+1=",
            "\\a^2+16=kN",
        ] {
            let (src, err) = eval_error(keys);
            // unit `kN` is a unit mismatch for plain numbers; only parse errors matter here
            assert!(xmath_core::parse_statement(&src).is_ok(), "{keys} -> {src}");
            if !keys.ends_with("kN") {
                assert!(err.is_none(), "{keys} -> {src}: {err:?}");
            }
        }
        let (src, _) = eval_error("\\a^2+16=");
        assert_eq!(src, "sqrt(a^(2 + 16)) =");
    }

    #[test]
    fn matrix_leaf_is_kept() {
        let e = Ed::from_source("B := [1, 2; 3, 4]*x").unwrap();
        assert_eq!(e.matrix_order().len(), 1);
        assert_eq!(e.source(), "B := [1, 2; 3, 4]*x");
    }
}
