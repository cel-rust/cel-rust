use std::collections::HashSet;

use crate::common::ast::{Expr, IdedExpr};

/// A collection of all the references that an expression makes to variables and functions.
pub struct ExpressionReferences<'expr> {
    variables: HashSet<&'expr str>,
    functions: HashSet<&'expr str>,
}

impl ExpressionReferences<'_> {
    /// Returns true if the expression references the provided variable name.
    ///
    /// # Example
    /// ```rust
    /// # use cel::parser::Parser;
    /// let expression = Parser::new().parse("foo.bar == true").unwrap();
    /// let references = expression.references();
    /// assert!(references.has_variable("foo"));
    /// ```
    pub fn has_variable(&self, name: impl AsRef<str>) -> bool {
        self.variables.contains(name.as_ref())
    }

    /// Returns true if the expression references the provided function name.
    ///
    /// # Example
    /// ```rust
    /// # use cel::parser::Parser;
    /// let expression = Parser::new().parse("size(foo) > 0").unwrap();
    /// let references = expression.references();
    /// assert!(references.has_function("size"));
    /// ```
    pub fn has_function(&self, name: impl AsRef<str>) -> bool {
        self.functions.contains(name.as_ref())
    }

    /// Returns a list of all variables referenced in the expression, which are
    /// the names to provide when evaluating it: what a comprehension binds, as
    /// the `map` macro does, isn't one. As it is told apart by name alone, the
    /// first segment of a namespaced function or a type name is one: `math` in
    /// `math.abs(a)`, or `int` in `type(a) == int`.
    ///
    /// # Example
    /// ```rust
    /// # use cel::parser::Parser;
    /// let expression = Parser::new().parse("foo.bar == true").unwrap();
    /// let references = expression.references();
    /// assert_eq!(vec!["foo"], references.variables());
    ///
    /// let expression = Parser::new().parse("[1].map(x, x + y)").unwrap();
    /// assert_eq!(vec!["y"], expression.references().variables());
    /// ```
    pub fn variables(&self) -> Vec<&str> {
        self.variables.iter().copied().collect()
    }

    /// Returns a list of all functions referenced in the expression.
    ///
    /// # Example
    /// ```rust
    /// # use cel::parser::Parser;
    /// let expression = Parser::new().parse("size(foo) > 0").unwrap();
    /// let references = expression.references();
    /// assert!(references.functions().contains(&"_>_"));
    /// assert!(references.functions().contains(&"size"));
    /// ```
    pub fn functions(&self) -> Vec<&str> {
        self.functions.iter().copied().collect()
    }
}

impl IdedExpr {
    /// Returns a set of all variables and functions referenced in the expression.
    ///
    /// # Example
    /// ```rust
    /// # use cel::parser::Parser;
    /// let expression = Parser::new().parse("foo && size(foo) > 0").unwrap();
    /// let references = expression.references();
    ///
    /// assert!(references.has_variable("foo"));
    /// assert!(references.has_function("size"));
    /// ```
    pub fn references(&self) -> ExpressionReferences<'_> {
        let mut variables = HashSet::new();
        let mut functions = HashSet::new();
        self._references(&mut variables, &mut functions, &mut Vec::new());
        ExpressionReferences {
            variables,
            functions,
        }
    }

    /// Internal recursive function to collect all variable and function references in the expression.
    /// `bound` holds the names the enclosing comprehensions bind: these aren't variables.
    fn _references<'expr>(
        &'expr self,
        variables: &mut HashSet<&'expr str>,
        functions: &mut HashSet<&'expr str>,
        bound: &mut Vec<&'expr str>,
    ) {
        match &self.expr {
            Expr::Unspecified => {}
            Expr::Call(call) => {
                functions.insert(without_leading_dot(&call.func_name));
                if let Some(target) = &call.target {
                    target._references(variables, functions, bound);
                }
                for arg in &call.args {
                    arg._references(variables, functions, bound);
                }
            }
            Expr::Comprehension(comp) => {
                comp.iter_range._references(variables, functions, bound);
                comp.accu_init._references(variables, functions, bound);
                let outer = bound.len();
                // The accumulator is in scope of the result, the iteration
                // variable of the loop only.
                bound.push(&comp.accu_var);
                comp.result._references(variables, functions, bound);
                bound.push(&comp.iter_var);
                comp.loop_cond._references(variables, functions, bound);
                comp.loop_step._references(variables, functions, bound);
                bound.truncate(outer);
            }
            Expr::Ident(name) => {
                // An absolute name, `.x`, doesn't match a bound one: it skips
                // the comprehensions' scopes. `@` can't be written in source.
                if !name.starts_with('@') && !bound.contains(&name.as_str()) {
                    variables.insert(without_leading_dot(name));
                }
            }
            Expr::List(list) => {
                for elem in &list.elements {
                    elem._references(variables, functions, bound);
                }
            }
            Expr::Literal(_) => {}
            Expr::Map(map) => {
                for entry in &map.entries {
                    match &entry.expr {
                        crate::common::ast::EntryExpr::StructField(field) => {
                            field.value._references(variables, functions, bound);
                        }
                        crate::common::ast::EntryExpr::MapEntry(map_entry) => {
                            map_entry.key._references(variables, functions, bound);
                            map_entry.value._references(variables, functions, bound);
                        }
                    }
                }
            }
            Expr::Select(select) => {
                select.operand._references(variables, functions, bound);
            }
            Expr::Struct(struct_expr) => {
                for entry in &struct_expr.entries {
                    match &entry.expr {
                        crate::common::ast::EntryExpr::StructField(field) => {
                            field.value._references(variables, functions, bound);
                        }
                        crate::common::ast::EntryExpr::MapEntry(map_entry) => {
                            map_entry.key._references(variables, functions, bound);
                            map_entry.value._references(variables, functions, bound);
                        }
                    }
                }
            }
        }
    }
}

/// The name a leading-dot reference (`.x`, `.f(1)`) is resolved by: the dot
/// only makes the name absolute, so it is not part of what the caller binds.
fn without_leading_dot(name: &str) -> &str {
    name.strip_prefix('.').unwrap_or(name)
}
