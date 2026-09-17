#![allow(clippy::module_inception)]
#[allow(clippy::all)]
mod gen;

pub mod references;

pub use crate::common::ast::IdedExpr as Expression;

mod macros;
mod parse;
#[allow(non_snake_case)]
mod parser;
#[cfg(feature = "parser_pratt")]
#[doc(hidden)]
pub mod pratt_parser;
#[cfg(feature = "parser_winnow")]
#[doc(hidden)]
pub mod winnow_parser;

pub use parser::*;
#[cfg(feature = "parser_pratt")]
#[doc(hidden)]
pub use pratt_parser::PrattParser;
pub use references::ExpressionReferences;
#[cfg(feature = "parser_winnow")]
#[doc(hidden)]
pub use winnow_parser::WinnowParser;
