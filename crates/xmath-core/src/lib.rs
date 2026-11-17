//! xmath-core: the calculation engine of xmath, a free Mathcad-like worksheet.
//!
//! The crate is UI-independent: it parses math regions, evaluates them in
//! worksheet order with physical units, and produces displayable results.

pub mod ast;
pub mod builtins;
pub mod eval;
pub mod format;
pub mod numeric;
pub mod parser;
pub mod units;
pub mod value;
pub mod worksheet;

pub use ast::{BinOp, Expr, Stmt, Target, UnOp};
pub use format::{FormatOptions, NumberText, Piece, ResultView, UnitPow, UnitText};
pub use parser::{parse_expr, parse_statement, ParseError};
pub use worksheet::{Output, PlotData, Region, RegionKind, RegionOutput, Series, Worksheet};

/// Evaluates on a thread with a large stack, so deeply recursive user
/// functions do not overflow the (small) main-thread stack.
pub fn evaluate_worksheet(ws: &Worksheet) -> std::collections::HashMap<u64, RegionOutput> {
    let ws = ws.clone();
    std::thread::Builder::new()
        .name("xmath-eval".into())
        .stack_size(512 * 1024 * 1024)
        .spawn(move || ws.evaluate())
        .expect("spawn evaluation thread")
        .join()
        .unwrap_or_default()
}
