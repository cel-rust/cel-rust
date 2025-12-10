pub mod ops;
pub mod value;

pub use value::BoxedValue;

use cel::common::ast::{Expr, IdedExpr};
use cel::{Context, ExecutionError, Value};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// Maximum number of fast slots for comprehension variables.
/// Slots 0-1 are typically used for accumulator and iteration variable.
const FAST_SLOTS: usize = 4;

/// Runtime context passed to compiled code.
///
/// This structure is passed as a pointer to all runtime functions,
/// providing access to CEL context (variables, functions) and error state.
#[repr(C)]
pub struct RuntimeContext<'a> {
    /// The CEL context containing variables and functions.
    pub cel_context: &'a Context<'a>,
    /// Error state - set by runtime functions when an error occurs.
    pub error: RefCell<Option<ExecutionError>>,
    /// Temporary variables for comprehension scope.
    /// These override the main context variables during comprehension evaluation.
    pub comprehension_vars: RefCell<HashMap<String, Value>>,
    /// Fast slot-based storage for comprehension variables (avoids HashMap overhead).
    /// Uses raw u64 to store BoxedValue representations directly.
    pub fast_slots: [Cell<u64>; FAST_SLOTS],
}

impl<'a> RuntimeContext<'a> {
    /// Create a new runtime context.
    pub fn new(cel_context: &'a Context<'a>) -> Self {
        RuntimeContext {
            cel_context,
            error: RefCell::new(None),
            comprehension_vars: RefCell::new(HashMap::new()),
            fast_slots: [Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)],
        }
    }

    /// Set a fast slot value (for comprehension variables).
    /// Slot 0 is typically the accumulator, slot 1 is the iteration variable.
    #[inline]
    pub fn set_fast_slot(&self, slot: usize, value: u64) {
        if slot < FAST_SLOTS {
            self.fast_slots[slot].set(value);
        }
    }

    /// Get a fast slot value.
    #[inline]
    pub fn get_fast_slot(&self, slot: usize) -> u64 {
        if slot < FAST_SLOTS {
            self.fast_slots[slot].get()
        } else {
            0
        }
    }

    /// Set an error.
    pub fn set_error(&self, error: ExecutionError) {
        *self.error.borrow_mut() = Some(error);
    }

    /// Take the error, if any.
    pub fn take_error(&self) -> Option<ExecutionError> {
        self.error.borrow_mut().take()
    }

    /// Check if an error has occurred.
    pub fn has_error(&self) -> bool {
        self.error.borrow().is_some()
    }

    /// Set a comprehension-scoped variable.
    pub fn set_comprehension_var(&self, name: &str, value: Value) {
        self.comprehension_vars.borrow_mut().insert(name.to_string(), value);
    }

    /// Get a comprehension-scoped variable.
    pub fn get_comprehension_var(&self, name: &str) -> Option<Value> {
        self.comprehension_vars.borrow().get(name).cloned()
    }

    /// Clear all comprehension variables.
    pub fn clear_comprehension_vars(&self) {
        self.comprehension_vars.borrow_mut().clear();
    }

    /// Call a registered function by name with pre-evaluated arguments.
    ///
    /// This creates a synthetic CallExpr and resolves it through the normal
    /// CEL resolution path, enabling support for user-registered functions.
    pub fn call_function(&self, name: &str, target: Option<Value>, args: Vec<Value>) -> Result<Value, ExecutionError> {
        use cel::common::ast::CallExpr;

        // Create a child context for temporary argument storage
        let mut child_ctx = self.cel_context.new_inner_scope();

        // Store all values as variables that can be resolved by identifier
        // Also store target if present
        if let Some(ref t) = target {
            child_ctx.add_variable_from_value("__jit_target", t.clone());
        }
        for (i, arg) in args.iter().enumerate() {
            let key = format!("__jit_arg_{}", i);
            child_ctx.add_variable_from_value(key, arg.clone());
        }

        // Create synthetic expressions that are identifiers pointing to our stored values
        let synthetic_target: Option<Box<IdedExpr>> = target.as_ref().map(|_| {
            Box::new(IdedExpr {
                id: 0,
                expr: Expr::Ident("__jit_target".to_string()),
            })
        });

        let synthetic_args: Vec<IdedExpr> = args
            .iter()
            .enumerate()
            .map(|(i, _)| IdedExpr {
                id: (i + 1) as u64,
                expr: Expr::Ident(format!("__jit_arg_{}", i)),
            })
            .collect();

        // Create a synthetic CallExpr
        let call_expr = CallExpr {
            func_name: name.to_string(),
            target: synthetic_target,
            args: synthetic_args,
        };

        // Wrap in IdedExpr and resolve
        let expr = IdedExpr {
            id: 1000,
            expr: Expr::Call(call_expr),
        };

        child_ctx.resolve(&expr)
    }
}

/// Result type for runtime functions.
/// Returns (value, error_flag) where error_flag is 0 for success, non-zero for error.
///
/// This struct is FFI-safe, unlike a tuple.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RuntimeResult {
    /// The result value (as a BoxedValue raw representation).
    pub value: u64,
    /// Error flag: 0 for success, 1 for error.
    pub error: u64,
}

impl RuntimeResult {
    /// Create a new success result.
    #[inline]
    pub const fn ok(value: u64) -> Self {
        RuntimeResult { value, error: 0 }
    }

    /// Create a new error result.
    #[inline]
    pub const fn err() -> Self {
        RuntimeResult { value: 0, error: 1 }
    }
}

/// Success result - value with no error.
#[inline]
pub fn rt_ok(value: BoxedValue) -> RuntimeResult {
    RuntimeResult::ok(value.as_raw())
}

/// Error result - sets error flag.
#[inline]
pub fn rt_err() -> RuntimeResult {
    RuntimeResult::err()
}
