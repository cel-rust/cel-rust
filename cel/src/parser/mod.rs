#![allow(clippy::module_inception)]
#[allow(clippy::all)]
mod gen;

pub mod ast;

pub mod common;
pub mod reference;
pub mod references;

pub use ast::IdedExpr as Expression;

mod macros;
mod parse;
#[allow(non_snake_case)]
mod parser;

pub use parser::*;
pub use references::ExpressionReferences;
