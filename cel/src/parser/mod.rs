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

pub use macros::Macro;
pub(crate) use macros::{
    all_comprehension, exists_comprehension, exists_one_comprehension, extract_ident,
    list_comprehension, map_macro_expander, Macros, ACCUMULATOR,
};
pub use parser::*;
#[cfg(feature = "parser_pratt")]
#[doc(hidden)]
pub use pratt_parser::PrattParser;
pub use references::ExpressionReferences;
