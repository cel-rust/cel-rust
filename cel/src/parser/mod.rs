#[allow(clippy::all)]
#[allow(clippy::module_inception)]
mod gen;

pub mod ast;
pub mod reference;
pub mod references;

pub use ast::IdedExpr as Expression;

mod macros;
mod parse;
#[allow(non_snake_case)]
mod parser;

pub use parser::*;
pub use references::ExpressionReferences;
