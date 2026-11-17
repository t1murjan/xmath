//! Opening Mathcad 15 worksheets (.xmcd).

use xmath_core::Worksheet;

/// Converts a Mathcad 15 `.xmcd` document; returns the worksheet and warnings.
pub fn xmcd(xml: &str) -> Result<(Worksheet, Vec<String>), String> {
    xmath_core::xmcd::import_xmcd(xml).map(|r| (r.worksheet, r.warnings))
}
