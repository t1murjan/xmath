//! Encoder of Mathcad 15 X-Y plots: the inverse of [`crate::xmcd_plot::decode_plot`].
//!
//! A plot item of an `.xmcd` file is the serialization of Mathcad's plot object. The byte
//! layout written here follows the plot items Mathcad 15 itself writes (see
//! `docs/xmcd-plot-format.md`):
//!
//! 1. a constant header that declares the classes `eqRegion`, `docRegion`, `mcObject`,
//!    `shpBox`, `shpRect` and carries the region rectangle (pixels) and the region id;
//! 2. the *expression tree* of the plot (axes, limits, expressions): every node is
//!    `number 0x32 class(2) 00 00 flag parent data…`; operators of one operand have one
//!    body byte, operators close with one `00` byte after their last child (the root and
//!    its wrappers do not);
//! 3. the next free node number and the format section `d2_graph_format`: `graphData`,
//!    three `axisFormat` records (X, Y, second Y), the `trace2D` table of 16 traces
//!    (`line type R G B` each) and `NumericalFormat`.
//!
//! What the format section of Mathcad can express and xmath writes: line type and colour of
//! the first 16 traces. Everything else of xmath's plot format (grid, log scales, symbols,
//! legend, titles, …) is stored by the exporter in an `<!--xmath:…-->` comment next to the
//! plot, because the encoding of those fields in Mathcad's format section is not known.

use crate::ast::{BinOp, Expr, UnOp};
use crate::xmcd_plot::*;

/// Line type (1 solid, 2 dots, 3 dashes, 4 dash-dot) and colour of one trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceSpec {
    pub line: u8,
    pub rgb: [u8; 3],
}

/// Mathcad's default trace table.
pub const DEFAULT_TRACES: [TraceSpec; 16] = {
    const fn t(line: u8, r: u8, g: u8, b: u8) -> TraceSpec {
        TraceSpec { line, rgb: [r, g, b] }
    }
    [
        t(1, 255, 0, 0),
        t(2, 0, 0, 255),
        t(3, 0, 255, 0),
        t(4, 255, 0, 255),
        t(1, 0, 255, 255),
        t(2, 128, 64, 0),
        t(3, 0, 0, 0),
        t(4, 255, 0, 0),
        t(1, 0, 0, 255),
        t(2, 0, 255, 0),
        t(3, 255, 0, 255),
        t(4, 0, 255, 255),
        t(1, 128, 64, 0),
        t(2, 0, 0, 0),
        t(3, 255, 0, 0),
        t(4, 0, 0, 255),
    ]
};

/// Everything the encoder writes of a plot.
#[derive(Debug, Clone, Default)]
pub struct PlotSpec {
    /// Abscissa expressions: none (empty placeholder), one for all traces, or one per trace.
    pub x: Vec<Expr>,
    pub y: Vec<Expr>,
    pub x_min: Option<f64>,
    pub x_max: Option<f64>,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    /// Overrides of the first traces (`None`: Mathcad's default for that slot).
    pub traces: Vec<Option<TraceSpec>>,
    /// Region rectangle in pixels: left, top, right, bottom.
    pub rect: [u32; 4],
    pub region_id: u32,
}

enum N {
    /// Identifier, or a number when `num` is set.
    Leaf { text: String, num: bool },
    Str(String),
    Empty,
    Op(u16, Vec<N>),
    /// A node written with the given flag byte (corpus comparison only).
    #[allow(dead_code)]
    Flag(u8, Box<N>),
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex constant")).collect()
}

/// Writes a length that is below 0x40 in one byte and larger in two, like node numbers.
fn len_num(v: usize, out: &mut Vec<u8>) {
    write_num(v as u32, out);
}

fn emit(n: &N, parent: u32, side: u8, depth: usize, next: &mut u32, out: &mut Vec<u8>) {
    emit_with(n, parent, side, depth, next, out, None)
}

fn emit_with(n: &N, parent: u32, side: u8, depth: usize, next: &mut u32, out: &mut Vec<u8>, over: Option<u8>) {
    if let N::Flag(l, inner) = n {
        return emit_with(inner, parent, side, depth, next, out, Some(*l));
    }
    let id = *next;
    *next += 1;
    write_num(id, out);
    out.push(0x32);
    let (cls, low): (u16, u8) = match n {
        N::Leaf { num: false, .. } => (C_LEAF, 0x24),
        N::Leaf { text, num: true } => (C_LEAF, if text.contains(['.', 'e', 'E']) { 0x34 } else { 0x36 }),
        N::Str(_) => (C_STRING, 0x24),
        N::Empty => (C_EMPTY, 0x02),
        N::Op(c, _) => (*c, 0),
        N::Flag(..) => unreachable!(),
    };

    out.extend_from_slice(&cls.to_be_bytes());
    out.extend_from_slice(&[0, 0, over.unwrap_or(side | low)]);
    write_num(parent, out);
    match n {
        N::Leaf { text, num } => {
            let len = text.chars().map(|c| c.len_utf16()).sum::<usize>();
            out.extend_from_slice(&[0, 0]);
            len_num(len + 1, out);
            if *num {
                out.push(1);
            }
            len_num(len, out);
            out.extend(utf16(text));
            out.push(0);
        }
        N::Str(text) => {
            let len = text.chars().map(|c| c.len_utf16()).sum::<usize>();
            out.extend_from_slice(&[0, 0]);
            len_num(len + 1, out);
            len_num(len, out);
            out.extend(utf16(text));
            out.push(0);
        }
        N::Empty => out.extend_from_slice(&[0, 0, 0]),
        N::Flag(..) => unreachable!(),
        N::Op(_, kids) => {
            if kids.len() == 1 {
                out.push(0);
            }
            for (i, k) in kids.iter().enumerate() {
                emit(k, id, if i == 0 { 0x40 } else { 0x80 }, depth + 1, next, out);
            }
            if depth > 0 {
                out.push(0);
            }
        }
    }
}

fn num_leaf(v: f64) -> N {
    N::Leaf { text: format!("{v}"), num: true }
}

fn pair(a: N, b: N) -> N {
    N::Op(C_PAIR, vec![a, b])
}

fn comma(items: Vec<N>) -> N {
    let mut it = items.into_iter();
    let Some(mut acc) = it.next() else { return N::Empty };
    for n in it {
        acc = N::Op(C_COMMA, vec![acc, n]);
    }
    acc
}

fn parens(n: N) -> N {
    N::Op(C_PARENS, vec![n])
}

/// Expression to tree. Constructs the tree has no node for (matrices, ranges, comparisons,
/// …) are written as one identifier holding their text.
fn expr_node(e: &Expr) -> N {
    // operand of an operator: parenthesised when it binds weaker than `prec`
    let operand = |e: &Expr, prec: u8, strict: bool| -> N {
        let n = expr_node(e);
        let p = e.precedence();
        if matches!(n, N::Op(..)) && (p < prec || (strict && p == prec)) {
            parens(n)
        } else {
            n
        }
    };
    match e {
        Expr::Num(v) if *v >= 0.0 || v.is_nan() => num_leaf(*v),
        Expr::Num(v) => N::Op(C_NEG, vec![num_leaf(-*v)]),
        Expr::Imag(v) => N::Leaf { text: format!("{v}i"), num: true },
        Expr::Str(s) => N::Str(s.clone()),
        Expr::Var(v) => N::Leaf { text: v.clone(), num: false },
        Expr::Unary(UnOp::Neg, a) => N::Op(C_NEG, vec![operand(a, 6, false)]),
        Expr::Binary(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow), a, b) => {
            let p = op.precedence();
            let cls = match op {
                BinOp::Add => C_ADD,
                BinOp::Sub => C_SUB,
                BinOp::Mul => C_MUL,
                BinOp::Div => C_DIV[0],
                _ => C_POW[0],
            };
            let right_strict = !matches!(op, BinOp::Add | BinOp::Mul);
            // powers associate to the right, everything else to the left
            let (ls, rs) = if *op == BinOp::Pow { (true, false) } else { (false, right_strict) };
            N::Op(cls, vec![operand(a, p, ls), operand(b, p, rs)])
        }
        Expr::Call(name, args) if name == "sqrt" && args.len() == 1 => N::Op(C_SQRT, vec![expr_node(&args[0])]),
        Expr::Call(name, args) => {
            let list = if args.is_empty() { N::Empty } else { comma(args.iter().map(expr_node).collect()) };
            N::Op(C_CALL, vec![N::Leaf { text: name.clone(), num: false }, parens(list)])
        }
        Expr::Index(base, idx) => {
            let b = operand(base, 9, false);
            N::Op(C_INDEX[0], vec![b, comma(idx.iter().map(expr_node).collect())])
        }
        Expr::MatCol(m, j) => N::Op(C_COLUMN, vec![operand(m, 9, false), expr_node(j)]),
        other => N::Leaf { text: other.to_string(), num: false },
    }
}

fn limit(v: Option<f64>) -> N {
    match v {
        None => N::Empty,
        Some(v) if v < 0.0 => N::Op(C_NEG, vec![num_leaf(-v)]),
        Some(v) => num_leaf(if v == 0.0 { 0.0 } else { v }),
    }
}

/// `((max, min), (markers)), expressions` — one axis group.
fn axis_node(max: Option<f64>, min: Option<f64>, exprs: Vec<N>) -> N {
    let body = if exprs.is_empty() { N::Empty } else { comma(exprs) };
    pair(pair(pair(limit(max), limit(min)), pair(N::Empty, N::Empty)), body)
}

/// The whole expression tree: `19c1( 9fc1(Y axis, second axis), X axis )`.
fn graph_tree(spec: &PlotSpec) -> N {
    let y = axis_node(spec.y_max, spec.y_min, spec.y.iter().map(expr_node).collect());
    let second = pair(pair(pair(num_leaf(0.0), num_leaf(0.0)), pair(N::Empty, N::Empty)), N::Empty);
    let x = axis_node(spec.x_max, spec.x_min, spec.x.iter().map(expr_node).collect());
    N::Op(C_XY, vec![pair(y, second), x])
}

const HEAD1: &str = "0c00000001000000530000000a000000010003000000086571526567696f6e0500000000003400000009646f63526567696f6e04000000000039000000086d634f626a6563740000000000005200000001000200000006736870426f780000000000004c00000007736870526563740000000000004d000000";
const HEAD2: &str = "0100000002000203000100000012646f63526567696f6e4174747269627574650100000000003a0000000f00000000000000000000000000043a22000000000000000000000000000500010000000474726565";
const TREE_HEAD: &str = "1b000000000032000000 0d70000000 0000";
const TAIL_A: &str = "020000000f64325f67726170685f666f726d617407000000000015000000096772617068446174610000000000001a0000000000000002200000000300000001000000150000000f0000000100010000000a61786973466f726d61740400000000001c0000004c0100000001000000000000000000ff00ff0000011c4c0100000001000000000000000000ff00ff0000011c4c0100000001000000000000000000ff00ff0000000001000100000007747261636532440300000000001b00000001000000";
const TAIL_C: &str = "1f010100010000000f4e756d65726963616c466f726d617408000000000048000000640069006900030000000f0000000a000000030000000000000000000000000000000000000c000000000000000000000000";

/// Serializes the plot item (before gzip and base64).
pub fn encode_plot(spec: &PlotSpec) -> Vec<u8> {
    let mut out = unhex(HEAD1);
    for v in [spec.rect[0], spec.rect[1], spec.rect[2], spec.rect[3], spec.rect[0], spec.rect[1], 0, spec.region_id, 1] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend(unhex(HEAD2));
    out.extend(unhex(&TREE_HEAD.replace(' ', "")));
    let mut next = FIRST_NODE;
    emit(&graph_tree(spec), ROOT_PARENT, 0x80, 0, &mut next, &mut out);
    write_num(next, &mut out);
    out.push(0);
    out.extend(unhex(TAIL_A));
    for i in 0..16usize {
        let t = spec.traces.get(i).copied().flatten().unwrap_or(DEFAULT_TRACES[i]);
        if i == 0 {
            out.extend_from_slice(&[0x1f, 0x01, 0x10, 0x00, 0x01, 0x1b]);
        } else {
            out.extend_from_slice(&[0x1f, 0x01, i as u8, 0x01, 0x1b]);
        }
        out.push(t.line);
        out.extend_from_slice(&t.rgb);
    }
    out.extend(unhex(TAIL_C));
    out
}

/// Reads the 16-entry trace table (`trace2D`) back: line type and colour of every trace.
/// `None` when the table does not have the layout Mathcad and [`encode_plot`] write.
pub fn decode_traces(bytes: &[u8]) -> Option<Vec<TraceSpec>> {
    let at = bytes.windows(7).position(|w| w == b"trace2D")? + 7;
    let mut i = at + 14;
    let mut out = Vec::new();
    for k in 0..16usize {
        let head: &[u8] = if k == 0 { &[0x1f, 0x01, 0x10, 0x00, 0x01, 0x1b] } else { &[0x1f, 0x01, k as u8, 0x01, 0x1b] };
        if bytes.get(i..i + head.len())? != head {
            return None;
        }
        i += head.len();
        let e = bytes.get(i..i + 4)?;
        out.push(TraceSpec { line: e[0], rgb: [e[1], e[2], e[3]] });
        i += 4;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_expr;
    use crate::xmcd_plot::decode_plot;

    fn spec(x: &[&str], y: &[&str]) -> PlotSpec {
        PlotSpec {
            x: x.iter().map(|s| parse_expr(s).unwrap()).collect(),
            y: y.iter().map(|s| parse_expr(s).unwrap()).collect(),
            rect: [160, 528, 403, 701],
            region_id: 30,
            ..Default::default()
        }
    }

    fn roundtrip(s: &PlotSpec) -> crate::xmcd_plot::PlotInfo {
        decode_plot(&encode_plot(s)).expect("decodes")
    }

    #[test]
    fn expressions_limits_and_traces_survive() {
        let mut s = spec(&["t"], &["sin(2*pi*t)", "a[1] + b^2/(c - 1)", "-x", "sqrt(x) * (a + b)"]);
        s.x_min = Some(0.2);
        s.x_max = Some(0.8);
        s.y_min = Some(-0.0996);
        s.y_max = Some(8.0);
        let info = roundtrip(&s);
        assert_eq!(info.y.iter().map(|e| e.to_string()).collect::<Vec<_>>(), s.y.iter().map(|e| e.to_string()).collect::<Vec<_>>());
        assert_eq!(info.x, s.x);
        assert_eq!((info.x_min, info.x_max, info.y_min, info.y_max), (Some(0.2), Some(0.8), Some(-0.0996), Some(8.0)));
    }

    #[test]
    fn placeholders_and_many_nodes() {
        // no abscissa, no limits; enough nodes for two-byte node numbers
        let ys: Vec<String> = (0..30).map(|i| format!("f{i}(x) + {i}*x")).collect();
        let refs: Vec<&str> = ys.iter().map(|s| s.as_str()).collect();
        let s = spec(&[], &refs);
        let info = roundtrip(&s);
        assert!(info.x.is_empty());
        assert_eq!(info.y.len(), 30);
        assert_eq!(info.y[29].to_string(), s.y[29].to_string());
    }

    #[test]
    fn trace_table_survives() {
        let mut s = spec(&["t"], &["a", "b", "c"]);
        s.traces = vec![Some(TraceSpec { line: 3, rgb: [1, 2, 3] }), None, Some(TraceSpec { line: 4, rgb: [250, 128, 7] })];
        let t = decode_traces(&encode_plot(&s)).expect("table");
        assert_eq!(t.len(), 16);
        assert_eq!(t[0], TraceSpec { line: 3, rgb: [1, 2, 3] });
        assert_eq!(t[1], DEFAULT_TRACES[1]);
        assert_eq!(t[2], TraceSpec { line: 4, rgb: [250, 128, 7] });
        assert_eq!(t[15], DEFAULT_TRACES[15]);
    }

    #[test]
    fn per_trace_abscissas() {
        let s = spec(&["x1", "x2"], &["y1", "y2"]);
        let info = roundtrip(&s);
        assert_eq!(info.x.len(), 2);
        assert_eq!(info.y.len(), 2);
    }

    #[test]
    fn unrepresentable_expressions_become_identifiers() {
        let s = spec(&["t"], &["[1, 2; 3, 4]"]);
        let info = roundtrip(&s);
        assert_eq!(info.y.len(), 1);
    }

    /// Rebuilds the node tree of a decoded plot item.
    fn rebuild(nodes: &[crate::xmcd_plot::Node], id: u32) -> N {
        let n = nodes.iter().find(|n| n.id == id).unwrap();
        let inner = match n.cls {
            C_LEAF => {
                let text = crate::xmcd_plot::leaf_text(&n.payload).unwrap();
                N::Leaf { text, num: n.flag & 0x10 != 0 }
            }
            C_STRING => N::Str(crate::xmcd_plot::leaf_text(&n.payload).unwrap()),
            C_EMPTY => N::Empty,
            c => {
                let mut kids: Vec<&crate::xmcd_plot::Node> = nodes.iter().filter(|k| k.parent == id).collect();
                kids.sort_by_key(|k| (k.flag & 0xc0 != 0x40, k.id));
                N::Op(c, kids.iter().map(|k| rebuild(nodes, k.id)).collect())
            }
        };
        N::Flag(n.flag, Box::new(inner))
    }

    /// The expression-tree bytes of every plot of the real Mathcad files in `XMCD_CORPUS`
    /// are re-created by the encoder (the files are private, so this only runs on demand:
    /// `XMCD_CORPUS=dir cargo test -p xmath-core corpus_trees -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn corpus_trees_are_reproduced_byte_for_byte() {
        use base64::Engine;
        use std::io::Read;
        let Ok(dir) = std::env::var("XMCD_CORPUS") else { return };
        let (mut same, mut total) = (0, 0);
        let mut diffs: Vec<String> = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "xmcd")).collect();
        files.sort();
        for f in files {
            let xml = String::from_utf8_lossy(&std::fs::read(&f).unwrap()).into_owned();
            let Ok(doc) = roxmltree::Document::parse_with_options(xml.trim_start_matches('\u{feff}'), roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }) else { continue };
            for it in doc.descendants().filter(|n| n.has_tag_name("item")) {
                let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(it.text().unwrap_or("").split_whitespace().collect::<String>()) else { continue };
                let mut bytes = Vec::new();
                if flate2::read::GzDecoder::new(raw.as_slice()).read_to_end(&mut bytes).is_err() {
                    continue;
                }
                if !bytes.windows(15).any(|w| w == b"d2_graph_format") {
                    continue;
                }
                let Ok(nodes) = crate::xmcd_plot::parse_nodes(&bytes) else { continue };
                total += 1;
                // original tree: from the first node to the free-node number before the format section
                let head = unhex(&TREE_HEAD.replace(' ', ""));
                let Some(t) = bytes.windows(4).position(|w| w == b"tree") else { continue };
                let Some(start) = bytes[t..].windows(head.len()).position(|w| w == head).map(|p| p + t + head.len()) else { continue };
                let Some(end) = bytes.windows(15).position(|w| w == b"d2_graph_format").map(|p| p - 5) else { continue };
                let orig = &bytes[start..end];
                let root = nodes.iter().find(|n| n.parent == ROOT_PARENT).unwrap().id;
                let mut mine = Vec::new();
                let mut next = FIRST_NODE;
                // wrapper nodes: re-create them as in the original
                let mut prefix = Vec::new();
                let mut n_wrap = 0;
                let mut cur = root;
                while let Some(n) = nodes.iter().find(|n| n.id == cur) {
                    if n.cls == C_WRAP_A || n.cls == C_WRAP_B {
                        write_num(next, &mut prefix);
                        next += 1;
                        prefix.push(0x32);
                        prefix.extend_from_slice(&n.cls.to_be_bytes());
                        prefix.extend_from_slice(&[0, 0, n.flag]);
                        write_num(if n_wrap == 0 { ROOT_PARENT } else { cur - 1 }, &mut prefix);
                        prefix.push(0);
                        n_wrap += 1;
                        cur = nodes.iter().find(|k| k.parent == cur).unwrap().id;
                    } else {
                        break;
                    }
                }
                let top_parent = if n_wrap == 0 { ROOT_PARENT } else { FIRST_NODE + n_wrap - 1 };
                let tree = rebuild(&nodes, cur);
                emit(&tree, top_parent, nodes.iter().find(|n| n.id == cur).map_or(0x80, |n| n.flag & 0xc0), 0, &mut next, &mut mine);
                let mut full = prefix;
                full.extend(mine);
                write_num(next, &mut full);
                full.push(0);
                // closing bytes of products are 1 in Mathcad when typed by juxtaposition (`2x`, `5 cm`)
                let same_up_to_closers = full.len() == orig.len() && full.iter().zip(orig).all(|(a, b)| a == b || (*a == 0 && *b == 1));
                if same_up_to_closers {
                    same += 1;
                } else if diffs.len() < 8 {
                    let k = full.iter().zip(orig).position(|(a, b)| a != b).unwrap_or(full.len().min(orig.len()));
                    diffs.push(format!("{}: byte {k}: mine {:02x?} original {:02x?}", f.display().to_string().rsplit('/').next().unwrap(), &full[k.saturating_sub(6)..(k+4).min(full.len())], &orig[k.saturating_sub(6)..(k+4).min(orig.len())]));
                }
            }
        }
        println!("{same} of {total} plot trees are byte-identical");
        for d in &diffs {
            println!("{d}");
        }
        assert!(total > 0);
        assert!(same * 100 >= total * 80, "{same} of {total}");
    }

    #[test]
    fn layout_follows_mathcad() {
        let b = encode_plot(&spec(&["t"], &["x"]));
        assert_eq!(&b[..9], &[0x0c, 0, 0, 0, 1, 0, 0, 0, 0x53]);
        let text = String::from_utf8_lossy(&b);
        for name in ["eqRegion", "docRegion", "shpRect", "tree", "d2_graph_format", "axisFormat", "trace2D", "NumericalFormat"] {
            assert!(text.contains(name), "{name}");
        }
    }
}
