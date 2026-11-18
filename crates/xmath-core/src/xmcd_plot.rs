//! Decoder of Mathcad 15 X-Y plots stored in `.xmcd` files.
//!
//! A `<plot item-idref="N"/>` region points to a `<binaryContent><item>` that
//! holds (gzip-compressed, base64-encoded) a proprietary serialization of the
//! plot. Inside it is an *expression tree*: every node has a 2-byte class code,
//! a position flag (left/right child) and the number of its parent; the leaves
//! are identifiers, numbers and strings in UTF-16LE. The format is described in
//! `docs/xmcd-plot-format.md`.
//!
//! The decoder extracts the axis expressions and the axis limits. It never
//! panics; anything it does not understand is an `Err`, and the importer then
//! falls back to the picture Mathcad stored for the region.

use crate::ast::{BinOp, Expr, UnOp};

/// What was recovered from a plot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlotInfo {
    /// Abscissa expressions (empty: not given, i.e. QuickPlot).
    pub x: Vec<Expr>,
    /// Ordinate expressions, one per trace.
    pub y: Vec<Expr>,
    pub x_min: Option<f64>,
    pub x_max: Option<f64>,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) id: u32,
    pub(crate) cls: u16,
    pub(crate) flag: u8,
    pub(crate) parent: u32,
    /// Bytes between the node header and the next node.
    pub(crate) payload: Vec<u8>,
}

// Class codes of the expression tree (see docs/xmcd-plot-format.md).
pub(crate) const C_LEAF: u16 = 0x020f;
pub(crate) const C_STRING: u16 = 0x5a0f;
pub(crate) const C_EMPTY: u16 = 0x000f;
pub(crate) const C_PAIR: u16 = 0x9fc1;
pub(crate) const C_XY: u16 = 0x19c1;
pub(crate) const C_WRAP_A: u16 = 0x1340;
pub(crate) const C_WRAP_B: u16 = 0x1440;
pub(crate) const C_COMMA: u16 = 0x0ac3;
pub(crate) const C_PARENS: u16 = 0x8e70;
pub(crate) const C_CALL: u16 = 0x12ce;
pub(crate) const C_MUL: u16 = 0x06ca;
pub(crate) const C_DIV: [u16; 2] = [0x07fb, 0x07cb];
pub(crate) const C_POW: [u16; 2] = [0x05cc, 0x05fc];
pub(crate) const C_NEG: u16 = 0x954b;
pub(crate) const C_ADD: u16 = 0x89c7;
pub(crate) const C_SUB: u16 = 0x88c7;
pub(crate) const C_INDEX: [u16; 1] = [0x04fd];
pub(crate) const C_COLUMN: u16 = 0x34fd;
pub(crate) const C_SQRT: u16 = 0x9a7b;

pub(crate) const FIRST_NODE: u32 = 6;
pub(crate) const ROOT_PARENT: u32 = 5;

/// Variable-length number: values below 0x40 take one byte, larger ones two
/// (`0x40 | high 6 bits`, low byte).
fn read_num(d: &[u8], i: usize) -> Option<(u32, usize)> {
    let b = *d.get(i)?;
    if b < 0x40 {
        Some((b as u32, i + 1))
    } else if b < 0x80 {
        let lo = *d.get(i + 1)?;
        Some(((((b & 0x3f) as u32) << 8) | lo as u32, i + 2))
    } else {
        None
    }
}

pub(crate) fn write_num(v: u32, out: &mut Vec<u8>) {
    if v < 0x40 {
        out.push(v as u8);
    } else {
        out.push(0x40 | (v >> 8) as u8);
        out.push((v & 0xff) as u8);
    }
}

fn find(d: &[u8], pat: &[u8], from: usize) -> Option<usize> {
    if pat.is_empty() || from >= d.len() || d.len() - from < pat.len() {
        return None;
    }
    d[from..].windows(pat.len()).position(|w| w == pat).map(|p| p + from)
}

/// Splits the serialized tree into nodes. Nodes are numbered consecutively
/// from 6, so the end of one node is found by looking for the next number
/// followed by the node tag `0x32` and a plausible header.
pub(crate) fn parse_nodes(d: &[u8]) -> Result<Vec<Node>, String> {
    let tree = find(d, b"tree", 0).ok_or("нет дерева выражений")?;
    let mut start = {
        let mut pat = Vec::new();
        write_num(FIRST_NODE, &mut pat);
        pat.push(0x32);
        find(d, &pat, tree).ok_or("нет узлов дерева")?
    };
    let mut nodes: Vec<Node> = Vec::new();
    let mut id = FIRST_NODE;
    loop {
        // header: number, 0x32, class(2), 0 0, flag, parent
        let mut pos = start;
        let (n, p) = read_num(d, pos).ok_or("обрыв данных")?;
        if n != id || d.get(p) != Some(&0x32) {
            return Err("нарушена нумерация узлов".into());
        }
        pos = p + 1;
        if d.len() < pos + 6 {
            return Err("обрыв данных".into());
        }
        let cls = u16::from_be_bytes([d[pos], d[pos + 1]]);
        let flag = d[pos + 4];
        let (parent, body) = read_num(d, pos + 5).ok_or("обрыв данных")?;
        // the next node
        let mut next_pat = Vec::new();
        write_num(id + 1, &mut next_pat);
        next_pat.push(0x32);
        let mut from = body;
        let mut next = None;
        while let Some(c) = find(d, &next_pat, from) {
            if plausible_header(d, c + next_pat.len(), id + 1) {
                next = Some(c);
                break;
            }
            from = c + 1;
        }
        match next {
            Some(c) => {
                nodes.push(Node { id, cls, flag, parent, payload: d[body..c].to_vec() });
                start = c;
                id += 1;
            }
            None => {
                nodes.push(Node { id, cls, flag, parent, payload: d[body..].to_vec() });
                break;
            }
        }
        if nodes.len() > 100_000 {
            return Err("слишком много узлов".into());
        }
    }
    Ok(nodes)
}

/// `class(2) 0 0 flag parent` with an existing, smaller parent number.
fn plausible_header(d: &[u8], pos: usize, id: u32) -> bool {
    if d.len() < pos + 6 || d[pos + 2] != 0 || d[pos + 3] != 0 || d[pos + 4] == 0 {
        return false;
    }
    match read_num(d, pos + 5) {
        Some((parent, _)) => parent >= ROOT_PARENT && parent < id,
        None => false,
    }
}

struct Tree {
    nodes: Vec<Node>,
}

impl Tree {
    fn get(&self, id: u32) -> Option<&Node> {
        // ids are consecutive from FIRST_NODE
        id.checked_sub(FIRST_NODE).and_then(|i| self.nodes.get(i as usize))
    }

    /// Children in order: left (flag 0x40) first, then right (0x80).
    fn children(&self, id: u32) -> Vec<&Node> {
        let mut v: Vec<&Node> = self.nodes.iter().filter(|n| n.parent == id).collect();
        v.sort_by_key(|n| (n.flag & 0xc0 != 0x40, n.id));
        v
    }
}

/// Text of a leaf: `00 00 n [01 n-1] chars...` with `n-1` UTF-16LE characters.
pub(crate) fn leaf_text(payload: &[u8]) -> Option<String> {
    if payload.len() < 3 || payload[0] != 0 || payload[1] != 0 {
        return None;
    }
    let n = payload[2] as usize;
    if n == 0 {
        return Some(String::new());
    }
    let chars = n - 1;
    let at = if n >= 2 && payload.get(3) == Some(&1) && payload.get(4).map(|b| *b as usize) == Some(chars) { 5 } else { 4 };
    let bytes = payload.get(at..at + 2 * chars)?;
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    Some(String::from_utf16_lossy(&units))
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Binary(op, Box::new(a), Box::new(b))
}

impl Tree {
    /// Splits a comma tree into its items.
    fn list<'a>(&'a self, n: &'a Node, out: &mut Vec<&'a Node>) {
        if n.cls == C_COMMA {
            for c in self.children(n.id) {
                self.list(c, out);
            }
        } else {
            out.push(n);
        }
    }

    fn items<'a>(&'a self, n: &'a Node) -> Vec<&'a Node> {
        let mut v = Vec::new();
        self.list(n, &mut v);
        v
    }

    /// Converts an expression node; `None` for an empty placeholder.
    fn expr(&self, n: &Node) -> Result<Option<Expr>, String> {
        let ks = self.children(n.id);
        let two = |tree: &Tree| -> Result<(Expr, Expr), String> {
            if ks.len() != 2 {
                return Err(format!("операция {:04x} ожидает два операнда", n.cls));
            }
            let a = tree.expr(ks[0])?.ok_or("пустой операнд")?;
            let b = tree.expr(ks[1])?.ok_or("пустой операнд")?;
            Ok((a, b))
        };
        let one = |tree: &Tree| -> Result<Expr, String> {
            if ks.len() != 1 {
                return Err(format!("операция {:04x} ожидает один операнд", n.cls));
            }
            tree.expr(ks[0])?.ok_or_else(|| "пустой операнд".to_string())
        };
        let e = match n.cls {
            C_EMPTY => return Ok(None),
            C_LEAF => {
                let t = leaf_text(&n.payload).ok_or("повреждённый лист")?;
                if t.is_empty() {
                    return Ok(None);
                }
                let first = t.chars().next().unwrap_or(' ');
                if first.is_ascii_digit() || (first == '.' && t.len() > 1) {
                    if let Ok(v) = t.parse::<f64>() {
                        Expr::Num(v)
                    } else if let Some(v) = t.strip_suffix(['i', 'j']).and_then(|s| s.parse::<f64>().ok()) {
                        Expr::Imag(v)
                    } else {
                        return Err(format!("непонятное число «{t}»"));
                    }
                } else {
                    Expr::Var(t)
                }
            }
            C_STRING => Expr::Str(leaf_text(&n.payload).ok_or("повреждённая строка")?),
            C_MUL => {
                let (a, b) = two(self)?;
                bin(BinOp::Mul, a, b)
            }
            c if C_DIV.contains(&c) => {
                let (a, b) = two(self)?;
                bin(BinOp::Div, a, b)
            }
            c if C_POW.contains(&c) => {
                let (a, b) = two(self)?;
                bin(BinOp::Pow, a, b)
            }
            C_ADD => {
                let (a, b) = two(self)?;
                bin(BinOp::Add, a, b)
            }
            C_SUB => {
                let (a, b) = two(self)?;
                bin(BinOp::Sub, a, b)
            }
            C_NEG => Expr::Unary(UnOp::Neg, Box::new(one(self)?)),
            C_SQRT => Expr::Call("sqrt".into(), vec![one(self)?]),
            C_PARENS => one(self)?,
            C_CALL => {
                if ks.len() != 2 {
                    return Err("вызов функции без аргументов".into());
                }
                let name = match self.expr(ks[0])? {
                    Some(Expr::Var(v)) => v,
                    _ => return Err("имя функции не идентификатор".into()),
                };
                Expr::Call(name, self.args(ks[1])?)
            }
            c if C_INDEX.contains(&c) => {
                if ks.len() != 2 {
                    return Err("индексация без индекса".into());
                }
                let base = self.expr(ks[0])?.ok_or("пустая основа индекса")?;
                Expr::Index(Box::new(base), self.args(ks[1])?)
            }
            C_COLUMN => {
                let (a, b) = two(self)?;
                Expr::Call("col".into(), vec![a, b])
            }
            other => return Err(format!("неизвестный узел {other:04x}")),
        };
        Ok(Some(e))
    }

    /// Arguments of a call or index: a comma list, possibly in a parens node.
    fn args(&self, n: &Node) -> Result<Vec<Expr>, String> {
        let mut inner = n;
        let kids;
        if n.cls == C_PARENS {
            kids = self.children(n.id);
            match kids.as_slice() {
                [] => return Ok(Vec::new()),
                [one] => inner = one,
                _ => return Err("скобки с несколькими выражениями".into()),
            }
        }
        let mut out = Vec::new();
        for item in self.items(inner) {
            out.push(self.expr(item)?.ok_or("пустой аргумент")?);
        }
        Ok(out)
    }

    /// Decodes an axis group: `(limits (max, min) / markers, expressions)`.
    fn axis(&self, n: &Node) -> Result<(Option<f64>, Option<f64>, Vec<Expr>), String> {
        let ks = self.children(n.id);
        if n.cls != C_PAIR || ks.len() != 2 {
            return Err("ось: неожиданная структура".into());
        }
        let lim = ks[0];
        let lks = self.children(lim.id);
        let pair = lks.first().ok_or("ось: нет пределов")?;
        let pks = self.children(pair.id);
        let value = |i: usize| -> Result<Option<f64>, String> {
            match pks.get(i) {
                None => Ok(None),
                Some(p) => Ok(self.expr(p).ok().flatten().and_then(|e| const_f64(&e))),
            }
        };
        let (max, min) = (value(0)?, value(1)?);
        let mut exprs = Vec::new();
        for item in self.items(ks[1]) {
            if let Some(e) = self.expr(item)? {
                exprs.push(e);
            }
        }
        Ok((max, min, exprs))
    }
}

/// Value of a constant expression of numbers.
pub fn const_f64(e: &Expr) -> Option<f64> {
    match e {
        Expr::Num(v) => Some(*v),
        Expr::Unary(UnOp::Neg, a) => const_f64(a).map(|v| -v),
        Expr::Binary(op, a, b) => {
            let (a, b) = (const_f64(a)?, const_f64(b)?);
            match op {
                BinOp::Add => Some(a + b),
                BinOp::Sub => Some(a - b),
                BinOp::Mul => Some(a * b),
                BinOp::Div => Some(a / b),
                BinOp::Pow => Some(a.powf(b)),
                _ => None,
            }
        }
        _ => None,
    }
    .filter(|v| v.is_finite())
}

/// Decodes the (already decompressed) plot item.
pub fn decode_plot(bytes: &[u8]) -> Result<PlotInfo, String> {
    let nodes = parse_nodes(bytes)?;
    let tree = Tree { nodes };
    for n in &tree.nodes {
        if n.parent != ROOT_PARENT && tree.get(n.parent).is_none() {
            return Err("узел без родителя".into());
        }
    }
    let mut root = tree.children(ROOT_PARENT).into_iter().next().ok_or("пустое дерево")?;
    while matches!(root.cls, C_WRAP_A | C_WRAP_B) {
        root = tree.children(root.id).into_iter().next().ok_or("пустая обёртка графика")?;
    }
    let mut info = PlotInfo::default();
    let y_axis: &Node;
    let kids;
    match root.cls {
        C_XY => {
            kids = tree.children(root.id);
            let y = kids.first().ok_or("график без осей")?;
            // Y group: (axis, secondary axis)
            let yk = tree.children(y.id);
            y_axis = yk.first().ok_or("нет оси Y")?;
            if let Some(xa) = kids.get(1) {
                let (max, min, exprs) = tree.axis(xa)?;
                (info.x_max, info.x_min, info.x) = (max, min, exprs);
            }
        }
        C_PAIR => y_axis = root,
        other => return Err(format!("неизвестный тип графика {other:04x}")),
    }
    let (max, min, exprs) = tree.axis(y_axis)?;
    (info.y_max, info.y_min, info.y) = (max, min, exprs);
    if info.y.is_empty() {
        return Err("нет выражений по оси Y".into());
    }
    // equal or reversed limits mean "automatic"
    for (lo, hi) in [(&mut info.x_min, &mut info.x_max), (&mut info.y_min, &mut info.y_max)] {
        if let (Some(a), Some(b)) = (*lo, *hi) {
            if a >= b {
                *lo = None;
                *hi = None;
            }
        }
    }
    Ok(info)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Tiny tree description for building synthetic plot bytes.
    pub(crate) enum T {
        Id(&'static str),
        Num(&'static str),
        Str(&'static str),
        Empty,
        Op(u16, Vec<T>),
    }

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    fn emit(t: &T, parent: u32, flag: u8, next: &mut u32, out: &mut Vec<u8>) {
        let id = *next;
        *next += 1;
        write_num(id, out);
        out.push(0x32);
        let cls = match t {
            T::Id(_) | T::Num(_) => C_LEAF,
            T::Str(_) => C_STRING,
            T::Empty => C_EMPTY,
            T::Op(c, _) => *c,
        };
        out.extend_from_slice(&cls.to_be_bytes());
        out.extend_from_slice(&[0, 0, flag]);
        write_num(parent, out);
        match t {
            T::Id(s) | T::Str(s) => {
                let n = s.chars().map(|c| c.len_utf16()).sum::<usize>();
                out.extend_from_slice(&[0, 0, (n + 1) as u8, n as u8]);
                out.extend(utf16(s));
                out.push(0);
            }
            T::Num(s) => {
                let n = s.len();
                out.extend_from_slice(&[0, 0, (n + 1) as u8, 1, n as u8]);
                out.extend(utf16(s));
                out.push(0);
            }
            T::Empty => out.extend_from_slice(&[0, 0, 0]),
            T::Op(_, kids) => {
                for (i, k) in kids.iter().enumerate() {
                    let f = if i == 0 { 0x40 } else { 0x80 };
                    emit(k, id, f, next, out);
                }
            }
        }
    }

    /// Serialized plot item with a fake header and trailing blob.
    pub(crate) fn build(root: &T) -> Vec<u8> {
        let mut out = vec![0x0c, 0, 0, 0, 1, 0, 0, 0, 0x53];
        out.extend_from_slice(&[0x04]);
        out.extend_from_slice(b"tree");
        out.extend_from_slice(&[0x1b, 0, 0, 0, 0, 0, 0x32, 0, 0, 0, 0x0d, 0x70, 0, 0, 0, 0, 0]);
        let mut next = FIRST_NODE;
        emit(root, ROOT_PARENT, 0x80, &mut next, &mut out);
        out.extend_from_slice(&[0, 0, 0x02, 0, 0, 0, 0x0f, b'd', b'2', b'_', b'g', b'r', b'a', b'p', b'h']);
        out
    }

    pub(crate) fn pair(a: T, b: T) -> T {
        T::Op(C_PAIR, vec![a, b])
    }

    /// `(axis limits, expressions)` in the shape Mathcad stores it.
    pub(crate) fn axis(max: T, min: T, exprs: T) -> T {
        pair(pair(pair(max, min), pair(T::Empty, T::Empty)), exprs)
    }

    pub(crate) fn comma(items: Vec<T>) -> T {
        let mut it = items.into_iter();
        let mut acc = it.next().unwrap();
        for n in it {
            acc = T::Op(C_COMMA, vec![acc, n]);
        }
        acc
    }

    fn xy(y: T, x: T) -> T {
        T::Op(C_XY, vec![pair(y, pair(pair(pair(T::Num("0"), T::Num("0")), pair(T::Empty, T::Empty)), T::Empty)), x])
    }

    #[test]
    fn decodes_traces_axes_and_limits() {
        let y = axis(
            // 2*10^(-6)
            T::Op(C_MUL, vec![T::Num("2"), T::Op(C_POW[0], vec![T::Num("10"), T::Op(C_NEG, vec![T::Num("6")])])]),
            T::Num("0"),
            comma(vec![T::Id("I1э"), T::Id("I2э"), T::Id("I3э")]),
        );
        let x = axis(T::Num("740"), T::Num("660"), T::Id("φ"));
        let info = decode_plot(&build(&xy(y, x))).unwrap();
        let ys: Vec<String> = info.y.iter().map(|e| e.to_string()).collect();
        assert_eq!(ys, ["I1э", "I2э", "I3э"]);
        assert_eq!(info.x, vec![Expr::Var("φ".into())]);
        assert_eq!((info.x_min, info.x_max), (Some(660.0), Some(740.0)));
        assert!((info.y_max.unwrap() - 2e-6).abs() < 1e-18);
        assert_eq!(info.y_min, Some(0.0));
    }

    #[test]
    fn function_calls_operators_and_subscripts() {
        let call = T::Op(C_CALL, vec![T::Id("zI_issl"), T::Op(C_PARENS, vec![T::Id("z")])]);
        let call2 = T::Op(C_CALL, vec![T::Id("f.a"), T::Op(C_PARENS, vec![comma(vec![T::Id("x"), T::Num("2.5")])])]);
        let sum = T::Op(C_ADD, vec![T::Id("yr"), T::Op(C_MUL, vec![T::Num("2"), T::Id("σ")])]);
        let ratio = T::Op(C_DIV[0], vec![T::Id("Q.Plant"), T::Id("cm")]);
        let y = axis(T::Empty, T::Empty, comma(vec![call, call2, sum, ratio]));
        let x = axis(T::Empty, T::Empty, comma(vec![T::Id("z"), T::Id("x"), T::Id("x"), T::Id("x")]));
        let info = decode_plot(&build(&xy(y, x))).unwrap();
        let ys: Vec<String> = info.y.iter().map(|e| e.to_string()).collect();
        assert_eq!(ys, ["zI_issl(z)", "f.a(x, 2.5)", "yr + 2*σ", "Q.Plant/cm"]);
        assert_eq!(info.x.len(), 4);
        assert_eq!((info.x_min, info.x_max, info.y_min, info.y_max), (None, None, None, None));
    }

    #[test]
    fn quickplot_root_and_wrappers() {
        let q = axis(T::Num("5"), T::Num("-5"), T::Op(C_CALL, vec![T::Id("f"), T::Op(C_PARENS, vec![T::Id("x")])]));
        let info = decode_plot(&build(&q)).unwrap();
        assert!(info.x.is_empty());
        assert_eq!(info.y[0].to_string(), "f(x)");
        assert_eq!(info.y_max, Some(5.0));
        let wrapped = T::Op(C_WRAP_A, vec![xy(axis(T::Empty, T::Empty, T::Id("v")), axis(T::Empty, T::Empty, T::Id("t")))]);
        let info = decode_plot(&build(&wrapped)).unwrap();
        assert_eq!(info.y[0].to_string(), "v");
        assert_eq!(info.x[0].to_string(), "t");
    }

    #[test]
    fn many_nodes_use_two_byte_numbers() {
        // more than 64 traces push node numbers past 0x3f
        let names: Vec<&'static str> = (0..40).map(|i| ["a", "b", "c", "d"][i % 4]).collect();
        let y = axis(T::Empty, T::Empty, comma(names.iter().map(|n| T::Id(n)).collect()));
        let x = axis(T::Empty, T::Empty, T::Str("txt"));
        let info = decode_plot(&build(&xy(y, x))).unwrap();
        assert_eq!(info.y.len(), 40);
        assert_eq!(info.y[39].to_string(), "d");
        assert_eq!(info.x, vec![Expr::Str("txt".into())]);
    }

    #[test]
    fn rejects_garbage_and_unknown_nodes() {
        assert!(decode_plot(b"").is_err());
        assert!(decode_plot(b"not a plot at all").is_err());
        let bad = axis(T::Empty, T::Empty, T::Op(0xe570, vec![T::Id("a")]));
        assert!(decode_plot(&build(&bad)).is_err());
        // no traces at all
        let empty = axis(T::Empty, T::Empty, T::Empty);
        assert!(decode_plot(&build(&empty)).is_err());
        // truncated
        let ok = build(&axis(T::Empty, T::Empty, T::Id("v")));
        for cut in [10, 30, ok.len() / 2] {
            let _ = decode_plot(&ok[..cut]);
        }
    }

    #[test]
    fn reversed_limits_are_ignored() {
        let q = axis(T::Num("1"), T::Num("3"), T::Id("v"));
        let info = decode_plot(&build(&q)).unwrap();
        assert_eq!((info.y_min, info.y_max), (None, None));
    }
}
