//! xmath-gui: the xmath desktop application and, without the `gui` feature, the page
//! typesetting and PDF export used by the command line tool and the MCP server.

#[cfg(feature = "gui")]
pub mod app;
pub mod canvas;
pub mod editor;
pub mod fonts;
pub mod image;
pub mod import;
pub mod mathview;
#[cfg(feature = "gui")]
pub mod palettes;
pub mod pdf;
pub mod plot;
pub mod print;
#[cfg(feature = "gui")]
pub mod uiscale;
