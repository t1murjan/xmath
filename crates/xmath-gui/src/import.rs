//! Opening Mathcad 15 worksheets (.xmcd).

use xmath_core::Worksheet;

/// Converts a Mathcad 15 `.xmcd` document; returns the worksheet and warnings.
pub fn xmcd(xml: &str) -> Result<(Worksheet, Vec<String>), String> {
    let _ = xml;
    Err("импорт .xmcd ещё не подключён".into())
}
