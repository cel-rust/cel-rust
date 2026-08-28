#![allow(clippy::module_inception)]
#[allow(clippy::all)]
mod gen;

pub mod references;

pub use crate::common::ast::IdedExpr as Expression;

mod macros;
mod parse;
#[allow(non_snake_case)]
mod parser;
#[doc(hidden)]
pub mod pratt_parser;

pub use parser::*;
#[doc(hidden)]
pub use pratt_parser::PrattParser;
pub use references::ExpressionReferences;

