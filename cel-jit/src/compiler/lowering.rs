//! CEL expression lowering to Cranelift IR.

use crate::error::CompileError;
use crate::runtime::BoxedValue;
use cel::common::ast::{operators, CallExpr, ComprehensionExpr, Expr, ListExpr, MapExpr, SelectExpr};
use cel::common::value::CelVal;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, InstBuilder, StackSlotData, StackSlotKind, Type, Value};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};
use std::collections::HashMap;

/// Integer arithmetic operations that can be inlined.
#[derive(Clone, Copy)]
enum IntOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// Integer comparison operations that can be inlined.
#[derive(Clone, Copy)]
enum IntCmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Try to extract an integer literal from an expression.
fn try_extract_int_literal(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(CelVal::Int(i)) => Some(*i),
        _ => None,
    }
}

/// Try to extract a boolean literal from an expression.
fn try_extract_bool_literal(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Literal(CelVal::Boolean(b)) => Some(*b),
        _ => None,
    }
}

/// String data collected during lowering that must outlive the compiled code.
pub struct LoweringData {
    pub string_constants: Vec<Box<str>>,
}

impl LoweringData {
    pub fn new() -> Self {
        LoweringData {
            string_constants: Vec::new(),
        }
    }
}

impl Default for LoweringData {
    fn default() -> Self {
        Self::new()
    }
}

/// Expression lowering context.
pub struct ExprLowerer<'a, 'b, M: Module> {
    builder: &'a mut FunctionBuilder<'b>,
    module: &'a mut M,
    runtime_funcs: &'a HashMap<&'static str, FuncId>,
    /// Runtime context pointer (parameter to the compiled function).
    ctx_param: Value,
    /// Pointer type for this target.
    ptr_type: Type,
    /// String data that must be kept alive for the lifetime of the compiled code.
    data: &'a mut LoweringData,
    /// Fast slot mappings for comprehension variables.
    /// Maps variable name to slot index (0-3).
    fast_slots: HashMap<String, u64>,
}

impl<'a, 'b, M: Module> ExprLowerer<'a, 'b, M> {
    pub fn new(
        builder: &'a mut FunctionBuilder<'b>,
        module: &'a mut M,
        runtime_funcs: &'a HashMap<&'static str, FuncId>,
        ctx_param: Value,
        ptr_type: Type,
        data: &'a mut LoweringData,
    ) -> Self {
        ExprLowerer {
            builder,
            module,
            runtime_funcs,
            ctx_param,
            ptr_type,
            data,
            fast_slots: HashMap::new(),
        }
    }

    /// Lower an expression, returning (value, error_flag) Cranelift values.
    pub fn lower_expr(&mut self, expr: &Expr) -> Result<(Value, Value), CompileError> {
        match expr {
            Expr::Literal(lit) => self.lower_literal(lit),
            Expr::Ident(name) => self.lower_ident(name),
            Expr::Call(call) => self.lower_call(call),
            Expr::Select(sel) => self.lower_select(sel),
            Expr::List(list) => self.lower_list(list),
            Expr::Map(map) => self.lower_map(map),
            Expr::Comprehension(comp) => self.lower_comprehension(comp),
            Expr::Struct(_) => Err(CompileError::UnsupportedExpression(
                "Struct expressions not yet supported".to_string(),
            )),
            Expr::Unspecified => Err(CompileError::UnsupportedExpression(
                "Unspecified expression".to_string(),
            )),
        }
    }

    /// Lower a literal value.
    fn lower_literal(&mut self, lit: &CelVal) -> Result<(Value, Value), CompileError> {
        let val = match lit {
            CelVal::Int(i) => {
                // Try to inline small integers
                if let Some(boxed) = BoxedValue::small_int(*i) {
                    self.builder.ins().iconst(types::I64, boxed.as_raw() as i64)
                } else {
                    // Box large integers via runtime call
                    let i_val = self.builder.ins().iconst(types::I64, *i);
                    self.call_runtime_single("rt_box_int", &[self.ctx_param, i_val])
                }
            }
            CelVal::UInt(u) => {
                // UInt always needs boxing (no inline representation)
                let u_val = self.builder.ins().iconst(types::I64, *u as i64);
                self.call_runtime_single("rt_box_uint", &[self.ctx_param, u_val])
            }
            CelVal::Double(f) => {
                let f_val = self.builder.ins().f64const(*f);
                self.call_runtime_single("rt_box_float", &[self.ctx_param, f_val])
            }
            CelVal::Boolean(b) => {
                let boxed = BoxedValue::bool(*b);
                self.builder.ins().iconst(types::I64, boxed.as_raw() as i64)
            }
            CelVal::Null => {
                let boxed = BoxedValue::null();
                self.builder.ins().iconst(types::I64, boxed.as_raw() as i64)
            }
            CelVal::String(s) => {
                // Store string constant and get pointer
                let (ptr, len) = self.string_constant(s);
                self.call_runtime_single("rt_box_string", &[self.ctx_param, ptr, len])
            }
            CelVal::Bytes(bytes) => {
                // Store bytes constant and get pointer
                let s = unsafe { std::str::from_utf8_unchecked(bytes) };
                let (ptr, len) = self.string_constant(s);
                self.call_runtime_single("rt_box_bytes", &[self.ctx_param, ptr, len])
            }
            _ => {
                return Err(CompileError::UnsupportedExpression(format!(
                    "Unsupported literal type: {:?}",
                    lit
                )))
            }
        };

        let no_error = self.builder.ins().iconst(types::I64, 0);
        Ok((val, no_error))
    }

    /// Lower an identifier (variable reference).
    /// Checks fast slots first for comprehension variables, then falls back to runtime lookup.
    fn lower_ident(&mut self, name: &str) -> Result<(Value, Value), CompileError> {
        // Check if this variable is in a fast slot (for comprehension variables)
        if let Some(&slot_idx) = self.fast_slots.get(name) {
            let slot = self.builder.ins().iconst(types::I64, slot_idx as i64);
            // Use rt_get_slot_cloned to get an owned copy that can be consumed by operations.
            // The original slot value remains intact for later references.
            let value = self.call_runtime_single("rt_get_slot_cloned", &[self.ctx_param, slot]);
            let no_error = self.builder.ins().iconst(types::I64, 0);
            return Ok((value, no_error));
        }

        // Fall back to runtime variable lookup
        let (ptr, len) = self.string_constant(name);
        self.call_runtime("rt_get_variable", &[self.ctx_param, ptr, len])
    }

    /// Lower a function call or operator.
    fn lower_call(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        match call.func_name.as_str() {
            // Binary arithmetic operators - use optimized inline versions
            operators::ADD => self.lower_int_binary_op_inline(call, IntOp::Add, "rt_add"),
            operators::SUBSTRACT => self.lower_int_binary_op_inline(call, IntOp::Sub, "rt_sub"),
            operators::MULTIPLY => self.lower_int_binary_op_inline(call, IntOp::Mul, "rt_mul"),
            operators::DIVIDE => self.lower_int_binary_op_inline(call, IntOp::Div, "rt_div"),
            operators::MODULO => self.lower_int_binary_op_inline(call, IntOp::Rem, "rt_rem"),

            // Comparison operators - use optimized inline versions
            operators::EQUALS => self.lower_int_cmp_inline(call, IntCmp::Eq, "rt_eq"),
            operators::NOT_EQUALS => self.lower_int_cmp_inline(call, IntCmp::Ne, "rt_ne"),
            operators::LESS => self.lower_int_cmp_inline(call, IntCmp::Lt, "rt_lt"),
            operators::LESS_EQUALS => self.lower_int_cmp_inline(call, IntCmp::Le, "rt_le"),
            operators::GREATER => self.lower_int_cmp_inline(call, IntCmp::Gt, "rt_gt"),
            operators::GREATER_EQUALS => self.lower_int_cmp_inline(call, IntCmp::Ge, "rt_ge"),

            // Logical operators (short-circuit)
            operators::LOGICAL_AND => self.lower_and(call),
            operators::LOGICAL_OR => self.lower_or(call),
            operators::LOGICAL_NOT => self.lower_not(call),

            // Unary negation - use optimized inline version
            operators::NEGATE => self.lower_int_neg_inline(call),

            // Ternary conditional
            operators::CONDITIONAL => self.lower_conditional(call),

            // Index operator
            operators::INDEX => self.lower_index(call),

            // In operator
            operators::IN => self.lower_in(call),

            // Internal operators used by comprehensions
            operators::NOT_STRICTLY_FALSE => self.lower_builtin_unary(call, "rt_not_strictly_false"),

            // Built-in functions
            "size" => self.lower_builtin_size(call),
            "contains" => self.lower_builtin_contains(call),
            "startsWith" => self.lower_builtin_method(call, "rt_starts_with"),
            "endsWith" => self.lower_builtin_method(call, "rt_ends_with"),
            "string" => self.lower_builtin_unary(call, "rt_string"),
            "int" => self.lower_builtin_unary(call, "rt_int"),
            "uint" => self.lower_builtin_unary(call, "rt_uint"),
            "double" => self.lower_builtin_unary(call, "rt_double"),
            "bytes" => self.lower_builtin_unary(call, "rt_bytes"),
            "type" => self.lower_builtin_unary(call, "rt_type"),
            "max" => self.lower_builtin_varargs(call, "rt_max"),
            "min" => self.lower_builtin_varargs(call, "rt_min"),

            // General function call
            _ => self.lower_function_call(call),
        }
    }

    // Tag constants for inline operations
    const TAG_MASK: i64 = 0b111;
    const TAG_SMALL_INT: i64 = 0b001;
    const TAG_BOOL: i64 = 0b010;

    /// Inline conversion to boolean with fast path for tagged booleans.
    /// Returns a Cranelift i64 value: 1 for true, 0 for false.
    fn inline_to_bool(&mut self, val: Value) -> Value {
        // Check if value is a tagged boolean
        let tag_mask = self.builder.ins().iconst(types::I64, Self::TAG_MASK);
        let bool_tag = self.builder.ins().iconst(types::I64, Self::TAG_BOOL);
        let tag = self.builder.ins().band(val, tag_mask);
        let is_bool = self.builder.ins().icmp(IntCC::Equal, tag, bool_tag);

        let fast_path = self.builder.create_block();
        let slow_path = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);

        self.builder.ins().brif(is_bool, fast_path, &[], slow_path, &[]);

        // Fast path: extract boolean directly (shift right 3)
        self.builder.switch_to_block(fast_path);
        self.builder.seal_block(fast_path);
        let three = self.builder.ins().iconst(types::I64, 3);
        let bool_val = self.builder.ins().ushr(val, three);
        self.builder.ins().jump(merge, &[bool_val]);

        // Slow path: call rt_to_bool
        self.builder.switch_to_block(slow_path);
        self.builder.seal_block(slow_path);
        let slow_result = self.call_runtime_single("rt_to_bool", &[self.ctx_param, val]);
        self.builder.ins().jump(merge, &[slow_result]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.builder.block_params(merge)[0]
    }

    /// Lower a binary operator.
    fn lower_binary_op(
        &mut self,
        call: &CallExpr,
        rt_func: &'static str,
    ) -> Result<(Value, Value), CompileError> {
        if call.args.len() != 2 {
            return Err(CompileError::Internal(format!(
                "Binary operator {} requires 2 arguments, got {}",
                rt_func,
                call.args.len()
            )));
        }

        // Lower left operand
        let (left, left_err) = self.lower_expr(&call.args[0].expr)?;

        // Check for error - short circuit if error
        let error_block = self.builder.create_block();
        let continue_block = self.builder.create_block();

        self.builder.ins().brif(left_err, error_block, &[], continue_block, &[]);

        // Error path - return the error
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Continue path - evaluate right operand
        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);

        let (right, right_err) = self.lower_expr(&call.args[1].expr)?;

        // Check right error
        let error_block2 = self.builder.create_block();
        let call_block = self.builder.create_block();

        self.builder.ins().brif(right_err, error_block2, &[], call_block, &[]);

        // Error path for right
        self.builder.switch_to_block(error_block2);
        self.builder.seal_block(error_block2);
        let zero2 = self.builder.ins().iconst(types::I64, 0);
        let one2 = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero2, one2]);

        // Call the runtime function
        self.builder.switch_to_block(call_block);
        self.builder.seal_block(call_block);

        let (result, err) = self.call_runtime(rt_func, &[self.ctx_param, left, right])?;
        self.builder.ins().jump(merge_block, &[result, err]);

        // Merge block
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower logical AND with short-circuit evaluation.
    fn lower_and(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        if call.args.len() != 2 {
            return Err(CompileError::Internal("AND requires 2 arguments".to_string()));
        }

        // Constant folding for boolean literals
        if let Some(left_lit) = try_extract_bool_literal(&call.args[0].expr) {
            if !left_lit {
                // false && _ = false
                let false_val = self.builder.ins().iconst(types::I64, BoxedValue::bool(false).as_raw() as i64);
                let no_err = self.builder.ins().iconst(types::I64, 0);
                return Ok((false_val, no_err));
            } else {
                // true && x = x
                return self.lower_expr(&call.args[1].expr);
            }
        }

        // Evaluate left
        let (left, left_err) = self.lower_expr(&call.args[0].expr)?;

        let error_block = self.builder.create_block();
        let check_left_block = self.builder.create_block();
        let false_block = self.builder.create_block();
        let right_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        // Check for error in left
        self.builder.ins().brif(left_err, error_block, &[], check_left_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Check if left is true - use inline fast path for booleans
        self.builder.switch_to_block(check_left_block);
        self.builder.seal_block(check_left_block);
        let left_bool = self.inline_to_bool(left);
        self.builder.ins().brif(left_bool, right_block, &[], false_block, &[]);

        // Left is false - return false
        self.builder.switch_to_block(false_block);
        self.builder.seal_block(false_block);
        let false_val = self.builder.ins().iconst(types::I64, BoxedValue::bool(false).as_raw() as i64);
        let no_err = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge_block, &[false_val, no_err]);

        // Evaluate right
        self.builder.switch_to_block(right_block);
        self.builder.seal_block(right_block);
        let (right, right_err) = self.lower_expr(&call.args[1].expr)?;
        self.builder.ins().jump(merge_block, &[right, right_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower logical OR with short-circuit evaluation.
    fn lower_or(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        if call.args.len() != 2 {
            return Err(CompileError::Internal("OR requires 2 arguments".to_string()));
        }

        // Constant folding for boolean literals
        if let Some(left_lit) = try_extract_bool_literal(&call.args[0].expr) {
            if left_lit {
                // true || _ = true
                let true_val = self.builder.ins().iconst(types::I64, BoxedValue::bool(true).as_raw() as i64);
                let no_err = self.builder.ins().iconst(types::I64, 0);
                return Ok((true_val, no_err));
            } else {
                // false || x = x
                return self.lower_expr(&call.args[1].expr);
            }
        }

        // Evaluate left
        let (left, left_err) = self.lower_expr(&call.args[0].expr)?;

        let error_block = self.builder.create_block();
        let check_left_block = self.builder.create_block();
        let true_block = self.builder.create_block();
        let right_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        // Check for error in left
        self.builder.ins().brif(left_err, error_block, &[], check_left_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Check if left is true - use inline fast path for booleans
        self.builder.switch_to_block(check_left_block);
        self.builder.seal_block(check_left_block);
        let left_bool = self.inline_to_bool(left);
        self.builder.ins().brif(left_bool, true_block, &[], right_block, &[]);

        // Left is true - return true
        self.builder.switch_to_block(true_block);
        self.builder.seal_block(true_block);
        let true_val = self.builder.ins().iconst(types::I64, BoxedValue::bool(true).as_raw() as i64);
        let no_err = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge_block, &[true_val, no_err]);

        // Evaluate right
        self.builder.switch_to_block(right_block);
        self.builder.seal_block(right_block);
        let (right, right_err) = self.lower_expr(&call.args[1].expr)?;
        self.builder.ins().jump(merge_block, &[right, right_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower logical NOT with inline fast path for booleans.
    fn lower_not(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        // Constant folding for boolean literals
        let operand_expr = if let Some(target) = &call.target {
            &target.expr
        } else if !call.args.is_empty() {
            &call.args[0].expr
        } else {
            return Err(CompileError::Internal("NOT requires operand".to_string()));
        };

        if let Some(b) = try_extract_bool_literal(operand_expr) {
            let result = self.builder.ins().iconst(types::I64, BoxedValue::bool(!b).as_raw() as i64);
            let no_err = self.builder.ins().iconst(types::I64, 0);
            return Ok((result, no_err));
        }

        // Lower the operand
        let (operand, err) = self.lower_expr(operand_expr)?;

        // Check for error
        let error_block = self.builder.create_block();
        let check_tag_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        self.builder.ins().brif(err, error_block, &[], check_tag_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Check if it's a tagged boolean
        self.builder.switch_to_block(check_tag_block);
        self.builder.seal_block(check_tag_block);

        let tag_mask = self.builder.ins().iconst(types::I64, Self::TAG_MASK);
        let bool_tag = self.builder.ins().iconst(types::I64, Self::TAG_BOOL);
        let tag = self.builder.ins().band(operand, tag_mask);
        let is_bool = self.builder.ins().icmp(IntCC::Equal, tag, bool_tag);

        let fast_path = self.builder.create_block();
        let slow_path = self.builder.create_block();
        self.builder.ins().brif(is_bool, fast_path, &[], slow_path, &[]);

        // Fast path: flip the boolean bit directly
        self.builder.switch_to_block(fast_path);
        self.builder.seal_block(fast_path);
        // The boolean value is in bit 3 (shifted left 3). XOR with 0b1000 to flip it.
        let flip_mask = self.builder.ins().iconst(types::I64, 0b1000);
        let result = self.builder.ins().bxor(operand, flip_mask);
        let no_err = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge_block, &[result, no_err]);

        // Slow path: call runtime
        self.builder.switch_to_block(slow_path);
        self.builder.seal_block(slow_path);
        let (slow_result, slow_err) = self.call_runtime("rt_not", &[self.ctx_param, operand])?;
        self.builder.ins().jump(merge_block, &[slow_result, slow_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower ternary conditional.
    fn lower_conditional(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        if call.args.len() != 3 {
            return Err(CompileError::Internal("Conditional requires 3 arguments".to_string()));
        }

        // Constant folding: if condition is a literal, just evaluate the appropriate branch
        if let Some(cond_lit) = try_extract_bool_literal(&call.args[0].expr) {
            if cond_lit {
                return self.lower_expr(&call.args[1].expr);
            } else {
                return self.lower_expr(&call.args[2].expr);
            }
        }

        // Evaluate condition
        let (cond, cond_err) = self.lower_expr(&call.args[0].expr)?;

        let error_block = self.builder.create_block();
        let check_block = self.builder.create_block();
        let true_block = self.builder.create_block();
        let false_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        // Check for error
        self.builder.ins().brif(cond_err, error_block, &[], check_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Check condition - use inline fast path for booleans
        self.builder.switch_to_block(check_block);
        self.builder.seal_block(check_block);
        let cond_bool = self.inline_to_bool(cond);
        self.builder.ins().brif(cond_bool, true_block, &[], false_block, &[]);

        // True branch
        self.builder.switch_to_block(true_block);
        self.builder.seal_block(true_block);
        let (true_val, true_err) = self.lower_expr(&call.args[1].expr)?;
        self.builder.ins().jump(merge_block, &[true_val, true_err]);

        // False branch
        self.builder.switch_to_block(false_block);
        self.builder.seal_block(false_block);
        let (false_val, false_err) = self.lower_expr(&call.args[2].expr)?;
        self.builder.ins().jump(merge_block, &[false_val, false_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower index operator.
    fn lower_index(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        self.lower_binary_op(call, "rt_index")
    }

    /// Lower 'in' operator.
    fn lower_in(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        self.lower_binary_op(call, "rt_in")
    }

    /// Lower a select expression (field access) or has() test.
    fn lower_select(&mut self, sel: &SelectExpr) -> Result<(Value, Value), CompileError> {
        let (target, target_err) = self.lower_expr(&sel.operand.expr)?;

        // Check for error
        let error_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        self.builder.ins().brif(target_err, error_block, &[], continue_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Continue path
        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);

        let (field_ptr, field_len) = self.string_constant(&sel.field);

        // If test is true, this is a has() check - use rt_has instead of rt_member
        let runtime_func = if sel.test { "rt_has" } else { "rt_member" };
        let (result, err) = self.call_runtime(runtime_func, &[self.ctx_param, target, field_ptr, field_len])?;
        self.builder.ins().jump(merge_block, &[result, err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower a list literal.
    fn lower_list(&mut self, list: &ListExpr) -> Result<(Value, Value), CompileError> {
        // Evaluate all elements
        let mut elements = Vec::with_capacity(list.elements.len());

        for elem in &list.elements {
            let (val, err) = self.lower_expr(&elem.expr)?;

            // Check for error - for simplicity, we'll abort on first error
            // TODO: More sophisticated error handling with proper control flow
            let error_block = self.builder.create_block();
            let continue_block = self.builder.create_block();

            self.builder.ins().brif(err, error_block, &[], continue_block, &[]);

            self.builder.switch_to_block(error_block);
            self.builder.seal_block(error_block);
            let zero = self.builder.ins().iconst(types::I64, 0);
            let one = self.builder.ins().iconst(types::I64, 1);
            self.builder.ins().return_(&[zero, one]);

            self.builder.switch_to_block(continue_block);
            self.builder.seal_block(continue_block);

            elements.push(val);
        }

        // Allocate stack space for elements array
        let stack_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (elements.len() * 8) as u32,
            8,
        ));

        // Store elements to stack
        for (i, elem) in elements.iter().enumerate() {
            let offset = (i * 8) as i32;
            self.builder.ins().stack_store(*elem, stack_slot, offset);
        }

        // Get pointer to stack array
        let ptr = self.builder.ins().stack_addr(self.ptr_type, stack_slot, 0);
        let len = self.builder.ins().iconst(types::I64, elements.len() as i64);

        self.call_runtime("rt_make_list", &[self.ctx_param, ptr, len])
    }

    /// Lower a map literal.
    fn lower_map(&mut self, map: &MapExpr) -> Result<(Value, Value), CompileError> {
        use cel::common::ast::EntryExpr;

        // Evaluate all entries
        let mut keys = Vec::with_capacity(map.entries.len());
        let mut values = Vec::with_capacity(map.entries.len());

        for entry in &map.entries {
            if let EntryExpr::MapEntry(map_entry) = &entry.expr {
                let (key_val, key_err) = self.lower_expr(&map_entry.key.expr)?;

                // Check for key error
                let error_block = self.builder.create_block();
                let continue_block = self.builder.create_block();

                self.builder.ins().brif(key_err, error_block, &[], continue_block, &[]);

                self.builder.switch_to_block(error_block);
                self.builder.seal_block(error_block);
                let zero = self.builder.ins().iconst(types::I64, 0);
                let one = self.builder.ins().iconst(types::I64, 1);
                self.builder.ins().return_(&[zero, one]);

                self.builder.switch_to_block(continue_block);
                self.builder.seal_block(continue_block);

                let (val_val, val_err) = self.lower_expr(&map_entry.value.expr)?;

                // Check for value error
                let error_block2 = self.builder.create_block();
                let continue_block2 = self.builder.create_block();

                self.builder.ins().brif(val_err, error_block2, &[], continue_block2, &[]);

                self.builder.switch_to_block(error_block2);
                self.builder.seal_block(error_block2);
                let zero2 = self.builder.ins().iconst(types::I64, 0);
                let one2 = self.builder.ins().iconst(types::I64, 1);
                self.builder.ins().return_(&[zero2, one2]);

                self.builder.switch_to_block(continue_block2);
                self.builder.seal_block(continue_block2);

                keys.push(key_val);
                values.push(val_val);
            }
        }

        // Allocate stack space for keys and values arrays
        let keys_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (keys.len() * 8) as u32,
            8,
        ));
        let values_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (values.len() * 8) as u32,
            8,
        ));

        // Store keys and values to stack
        for (i, key) in keys.iter().enumerate() {
            let offset = (i * 8) as i32;
            self.builder.ins().stack_store(*key, keys_slot, offset);
        }
        for (i, val) in values.iter().enumerate() {
            let offset = (i * 8) as i32;
            self.builder.ins().stack_store(*val, values_slot, offset);
        }

        // Get pointers to stack arrays
        let keys_ptr = self.builder.ins().stack_addr(self.ptr_type, keys_slot, 0);
        let values_ptr = self.builder.ins().stack_addr(self.ptr_type, values_slot, 0);
        let len = self.builder.ins().iconst(types::I64, keys.len() as i64);

        self.call_runtime("rt_make_map", &[self.ctx_param, keys_ptr, values_ptr, len])
    }

    /// Lower a comprehension expression.
    ///
    /// Comprehension structure:
    /// - iter_range: The collection to iterate over
    /// - iter_var: Variable name bound to each element
    /// - iter_var2: Optional second variable (for map key-value iteration)
    /// - accu_var: Accumulator variable name
    /// - accu_init: Initial value for accumulator
    /// - loop_cond: Condition to check before each iteration
    /// - loop_step: Expression to compute new accumulator value
    /// - result: Final result expression
    ///
    /// Optimization: Uses fast slots (array indices) instead of HashMap lookups
    /// for comprehension variables (accu_var at slot 0, iter_var at slot 1).
    fn lower_comprehension(&mut self, comp: &ComprehensionExpr) -> Result<(Value, Value), CompileError> {
        // Register comprehension variables in fast slots for optimized access
        // Slot 0: accu_var, Slot 1: iter_var
        const ACCU_SLOT: u64 = 0;
        const ITER_SLOT: u64 = 1;
        self.fast_slots.insert(comp.accu_var.clone(), ACCU_SLOT);
        self.fast_slots.insert(comp.iter_var.clone(), ITER_SLOT);

        // Merge block for final result
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        // 1. Evaluate initial accumulator value
        let (accu_init, accu_init_err) = self.lower_expr(&comp.accu_init.expr)?;

        // Check for error
        let error_block1 = self.builder.create_block();
        let continue1 = self.builder.create_block();
        self.builder.ins().brif(accu_init_err, error_block1, &[], continue1, &[]);

        self.builder.switch_to_block(error_block1);
        self.builder.seal_block(error_block1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        self.builder.switch_to_block(continue1);
        self.builder.seal_block(continue1);

        // 2. Evaluate the range (list/map to iterate)
        let (iter_range, iter_range_err) = self.lower_expr(&comp.iter_range.expr)?;

        // Check for error
        let error_block2 = self.builder.create_block();
        let continue2 = self.builder.create_block();
        self.builder.ins().brif(iter_range_err, error_block2, &[], continue2, &[]);

        self.builder.switch_to_block(error_block2);
        self.builder.seal_block(error_block2);
        let zero2 = self.builder.ins().iconst(types::I64, 0);
        let one2 = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero2, one2]);

        self.builder.switch_to_block(continue2);
        self.builder.seal_block(continue2);

        // Get the length of the range
        let range_len = self.call_runtime_single("rt_list_len", &[self.ctx_param, iter_range]);

        // Set initial accumulator using fast slot
        let accu_slot_const = self.builder.ins().iconst(types::I64, ACCU_SLOT as i64);
        self.call_runtime_void("rt_set_slot", &[self.ctx_param, accu_slot_const, accu_init]);

        // Prepare iter_var slot constant
        let iter_slot_const = self.builder.ins().iconst(types::I64, ITER_SLOT as i64);

        // Create loop blocks
        let loop_header = self.builder.create_block();
        let loop_body = self.builder.create_block();
        let loop_exit = self.builder.create_block();

        // Add parameters to loop header: (index, accumulator)
        // Using SSA for accumulator eliminates rt_get_slot call in loop header
        self.builder.append_block_param(loop_header, types::I64); // index
        self.builder.append_block_param(loop_header, types::I64); // accu (SSA)

        // Add parameter to loop_exit for final accu value
        self.builder.append_block_param(loop_exit, types::I64); // final_accu

        // Start the loop with index 0 and initial accumulator
        let zero_idx = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(loop_header, &[zero_idx, accu_init]);

        // Loop header: check if index < len
        self.builder.switch_to_block(loop_header);
        let index = self.builder.block_params(loop_header)[0];
        let current_accu = self.builder.block_params(loop_header)[1];

        let at_end = self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, index, range_len);
        self.builder.ins().brif(at_end, loop_exit, &[current_accu], loop_body, &[]);

        // Loop body
        self.builder.switch_to_block(loop_body);

        // Get current element
        let elem = self.call_runtime_single("rt_list_get", &[self.ctx_param, iter_range, index]);

        // Set iter_var using fast slot (much faster than HashMap lookup)
        self.call_runtime_void("rt_set_slot", &[self.ctx_param, iter_slot_const, elem]);

        // Set accu_var using fast slot for expression evaluation
        self.call_runtime_void("rt_set_slot", &[self.ctx_param, accu_slot_const, current_accu]);

        // Check loop condition
        let (loop_cond, loop_cond_err) = self.lower_expr(&comp.loop_cond.expr)?;

        // Check for error in condition
        let cond_error_block = self.builder.create_block();
        let cond_ok_block = self.builder.create_block();
        self.builder.ins().brif(loop_cond_err, cond_error_block, &[], cond_ok_block, &[]);

        self.builder.switch_to_block(cond_error_block);
        self.builder.seal_block(cond_error_block);
        // Free iter_range on error
        self.call_runtime_void("rt_free_value", &[self.ctx_param, iter_range]);
        let zero_err = self.builder.ins().iconst(types::I64, 0);
        let one_err = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero_err, one_err]);

        self.builder.switch_to_block(cond_ok_block);
        self.builder.seal_block(cond_ok_block);

        // Convert condition to bool - use inline fast path for booleans
        let cond_bool = self.inline_to_bool(loop_cond);

        // If condition is false, free the element and exit the loop
        let step_block = self.builder.create_block();
        let early_exit_block = self.builder.create_block();
        self.builder.ins().brif(cond_bool, step_block, &[], early_exit_block, &[]);

        // Early exit: free the iter_var element before exiting (iter_range freed at loop_exit)
        self.builder.switch_to_block(early_exit_block);
        self.builder.seal_block(early_exit_block);
        self.call_runtime_void("rt_free_value", &[self.ctx_param, elem]);
        self.builder.ins().jump(loop_exit, &[current_accu]);

        // Execute step expression
        self.builder.switch_to_block(step_block);
        self.builder.seal_block(step_block);

        let (step_result, step_err) = self.lower_expr(&comp.loop_step.expr)?;

        // Check for error in step
        let step_error_block = self.builder.create_block();
        let step_ok_block = self.builder.create_block();
        self.builder.ins().brif(step_err, step_error_block, &[], step_ok_block, &[]);

        self.builder.switch_to_block(step_error_block);
        self.builder.seal_block(step_error_block);
        // Free element and iter_range before error exit
        self.call_runtime_void("rt_free_value", &[self.ctx_param, elem]);
        self.call_runtime_void("rt_free_value", &[self.ctx_param, iter_range]);
        let zero_step = self.builder.ins().iconst(types::I64, 0);
        let one_step = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero_step, one_step]);

        self.builder.switch_to_block(step_ok_block);
        self.builder.seal_block(step_ok_block);

        // Free the iter_var element before continuing to next iteration
        self.call_runtime_void("rt_free_value", &[self.ctx_param, elem]);

        // Free the old accumulator if it's different from the new one.
        // This handles cases like filter/map where a new list is created each iteration.
        // We compare raw values - if they're the same, don't double-free.
        let accu_same = self.builder.ins().icmp(IntCC::Equal, current_accu, step_result);
        let free_old_accu_block = self.builder.create_block();
        let continue_loop_block = self.builder.create_block();
        self.builder.ins().brif(accu_same, continue_loop_block, &[], free_old_accu_block, &[]);

        self.builder.switch_to_block(free_old_accu_block);
        self.builder.seal_block(free_old_accu_block);
        self.call_runtime_void("rt_free_value", &[self.ctx_param, current_accu]);
        self.builder.ins().jump(continue_loop_block, &[]);

        self.builder.switch_to_block(continue_loop_block);
        self.builder.seal_block(continue_loop_block);

        // Increment index and continue loop with new accumulator (via SSA)
        let one_const = self.builder.ins().iconst(types::I64, 1);
        let next_index = self.builder.ins().iadd(index, one_const);
        self.builder.ins().jump(loop_header, &[next_index, step_result]);

        // Seal loop header and body after all predecessors are known
        self.builder.seal_block(loop_header);
        self.builder.seal_block(loop_body);

        // Loop exit
        self.builder.switch_to_block(loop_exit);
        self.builder.seal_block(loop_exit);

        // Get the final accumulator value (passed as parameter)
        let final_accu = self.builder.block_params(loop_exit)[0];

        // Free the iteration range (the source list/map) now that iteration is complete
        self.call_runtime_void("rt_free_value", &[self.ctx_param, iter_range]);

        // Update accu_var using fast slot with the final value (for the result expression)
        self.call_runtime_void("rt_set_slot", &[self.ctx_param, accu_slot_const, final_accu]);

        // Evaluate result expression
        let (result, result_err) = self.lower_expr(&comp.result.expr)?;

        // Free the final accumulator value after the result expression has been evaluated.
        // The result expression (typically __result__) creates a clone via rt_get_slot_cloned,
        // so we need to free the original value stored in the slot.
        // Only free if result != final_accu to avoid double-free in case result expression
        // returns the accumulator directly (though with rt_get_slot_cloned this shouldn't happen).
        let result_same_as_accu = self.builder.ins().icmp(IntCC::Equal, result, final_accu);
        let free_accu_block = self.builder.create_block();
        let skip_free_block = self.builder.create_block();
        self.builder.ins().brif(result_same_as_accu, skip_free_block, &[], free_accu_block, &[]);

        self.builder.switch_to_block(free_accu_block);
        self.builder.seal_block(free_accu_block);
        self.call_runtime_void("rt_free_value", &[self.ctx_param, final_accu]);
        self.builder.ins().jump(skip_free_block, &[]);

        self.builder.switch_to_block(skip_free_block);
        self.builder.seal_block(skip_free_block);

        // Clean up fast slots - remove comprehension variables
        self.fast_slots.remove(&comp.accu_var);
        self.fast_slots.remove(&comp.iter_var);

        self.builder.ins().jump(merge_block, &[result, result_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower a general function call (non-operator).
    fn lower_function_call(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        // Get function name
        let (name_ptr, name_len) = self.string_constant(&call.func_name);

        // Evaluate target if present
        let (target_val, has_target) = if let Some(target) = &call.target {
            let (val, err) = self.lower_expr(&target.expr)?;

            // Check for error
            let error_block = self.builder.create_block();
            let continue_block = self.builder.create_block();

            self.builder.ins().brif(err, error_block, &[], continue_block, &[]);

            self.builder.switch_to_block(error_block);
            self.builder.seal_block(error_block);
            let zero = self.builder.ins().iconst(types::I64, 0);
            let one = self.builder.ins().iconst(types::I64, 1);
            self.builder.ins().return_(&[zero, one]);

            self.builder.switch_to_block(continue_block);
            self.builder.seal_block(continue_block);

            let has_target = self.builder.ins().iconst(types::I64, 1);
            (val, has_target)
        } else {
            let zero = self.builder.ins().iconst(types::I64, 0);
            (zero, zero)
        };

        // Evaluate arguments
        let mut arg_vals = Vec::with_capacity(call.args.len());
        for arg in &call.args {
            let (val, err) = self.lower_expr(&arg.expr)?;

            // Check for error
            let error_block = self.builder.create_block();
            let continue_block = self.builder.create_block();

            self.builder.ins().brif(err, error_block, &[], continue_block, &[]);

            self.builder.switch_to_block(error_block);
            self.builder.seal_block(error_block);
            let zero = self.builder.ins().iconst(types::I64, 0);
            let one = self.builder.ins().iconst(types::I64, 1);
            self.builder.ins().return_(&[zero, one]);

            self.builder.switch_to_block(continue_block);
            self.builder.seal_block(continue_block);

            arg_vals.push(val);
        }

        // Allocate stack space for arguments array
        let args_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (arg_vals.len().max(1) * 8) as u32,
            8,
        ));

        // Store arguments to stack
        for (i, val) in arg_vals.iter().enumerate() {
            let offset = (i * 8) as i32;
            self.builder.ins().stack_store(*val, args_slot, offset);
        }

        // Get pointer to args array
        let args_ptr = self.builder.ins().stack_addr(self.ptr_type, args_slot, 0);
        let args_len = self.builder.ins().iconst(types::I64, arg_vals.len() as i64);

        // Call rt_call_function
        self.call_runtime(
            "rt_call_function",
            &[self.ctx_param, name_ptr, name_len, target_val, has_target, args_ptr, args_len],
        )
    }

    /// Lower a binary integer operation with inline fast path.
    /// If both operands are small integers (tagged), perform the operation inline.
    /// Otherwise, fall back to the runtime function.
    fn lower_int_binary_op_inline(
        &mut self,
        call: &CallExpr,
        op: IntOp,
        fallback: &'static str,
    ) -> Result<(Value, Value), CompileError> {
        if call.args.len() != 2 {
            return Err(CompileError::Internal(format!(
                "Binary operator requires 2 arguments, got {}",
                call.args.len()
            )));
        }

        // Constant folding: if both operands are literals, compute at compile time
        if let (Some(left_lit), Some(right_lit)) = (
            try_extract_int_literal(&call.args[0].expr),
            try_extract_int_literal(&call.args[1].expr),
        ) {
            let result = match op {
                IntOp::Add => left_lit.checked_add(right_lit),
                IntOp::Sub => left_lit.checked_sub(right_lit),
                IntOp::Mul => left_lit.checked_mul(right_lit),
                IntOp::Div => if right_lit != 0 { left_lit.checked_div(right_lit) } else { None },
                IntOp::Rem => if right_lit != 0 { left_lit.checked_rem(right_lit) } else { None },
            };

            if let Some(result_val) = result {
                // Return constant result
                let boxed = if let Some(b) = BoxedValue::small_int(result_val) {
                    self.builder.ins().iconst(types::I64, b.as_raw() as i64)
                } else {
                    // Large result - box it
                    let val = self.builder.ins().iconst(types::I64, result_val);
                    self.call_runtime_single("rt_box_int", &[self.ctx_param, val])
                };
                let no_error = self.builder.ins().iconst(types::I64, 0);
                return Ok((boxed, no_error));
            }
            // Overflow or division by zero - fall through to runtime for error handling
        }

        // Lower both operands first
        let (left, left_err) = self.lower_expr(&call.args[0].expr)?;

        // Check for error in left
        let left_error_block = self.builder.create_block();
        let left_ok_block = self.builder.create_block();
        self.builder.ins().brif(left_err, left_error_block, &[], left_ok_block, &[]);

        // Left error path
        self.builder.switch_to_block(left_error_block);
        self.builder.seal_block(left_error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Left OK - evaluate right
        self.builder.switch_to_block(left_ok_block);
        self.builder.seal_block(left_ok_block);

        let (right, right_err) = self.lower_expr(&call.args[1].expr)?;

        // Check for error in right
        let right_error_block = self.builder.create_block();
        let check_tags_block = self.builder.create_block();
        self.builder.ins().brif(right_err, right_error_block, &[], check_tags_block, &[]);

        // Right error path
        self.builder.switch_to_block(right_error_block);
        self.builder.seal_block(right_error_block);
        let zero2 = self.builder.ins().iconst(types::I64, 0);
        let one2 = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero2, one2]);

        // Check if both are small integers (tagged)
        self.builder.switch_to_block(check_tags_block);
        self.builder.seal_block(check_tags_block);

        let tag_mask = self.builder.ins().iconst(types::I64, Self::TAG_MASK);
        let small_int_tag = self.builder.ins().iconst(types::I64, Self::TAG_SMALL_INT);

        let left_tag = self.builder.ins().band(left, tag_mask);
        let right_tag = self.builder.ins().band(right, tag_mask);

        let left_is_small = self.builder.ins().icmp(IntCC::Equal, left_tag, small_int_tag);
        let right_is_small = self.builder.ins().icmp(IntCC::Equal, right_tag, small_int_tag);
        let both_small = self.builder.ins().band(left_is_small, right_is_small);

        let fast_path_block = self.builder.create_block();
        let slow_path_block = self.builder.create_block();
        self.builder.ins().brif(both_small, fast_path_block, &[], slow_path_block, &[]);

        // Fast path: both are small integers
        self.builder.switch_to_block(fast_path_block);
        self.builder.seal_block(fast_path_block);

        // Extract values (shift right 3 to remove tag, then sign extend)
        let three = self.builder.ins().iconst(types::I64, 3);
        let left_val = self.builder.ins().sshr(left, three);
        let right_val = self.builder.ins().sshr(right, three);

        // Perform the operation
        let result = match op {
            IntOp::Add => self.builder.ins().iadd(left_val, right_val),
            IntOp::Sub => self.builder.ins().isub(left_val, right_val),
            IntOp::Mul => self.builder.ins().imul(left_val, right_val),
            IntOp::Div => {
                // Division needs zero check - go to slow path for proper error handling
                self.builder.ins().jump(slow_path_block, &[]);
                self.builder.switch_to_block(slow_path_block);
                // This block will be sealed below
                let (result, err) = self.call_runtime(fallback, &[self.ctx_param, left, right])?;
                self.builder.ins().jump(merge_block, &[result, err]);

                // Continue without adding more code to fast path (we jumped away)
                self.builder.switch_to_block(merge_block);
                self.builder.seal_block(slow_path_block);
                self.builder.seal_block(merge_block);

                let result_val = self.builder.block_params(merge_block)[0];
                let error_val = self.builder.block_params(merge_block)[1];
                return Ok((result_val, error_val));
            }
            IntOp::Rem => {
                // Remainder needs zero check - go to slow path for proper error handling
                self.builder.ins().jump(slow_path_block, &[]);
                self.builder.switch_to_block(slow_path_block);
                let (result, err) = self.call_runtime(fallback, &[self.ctx_param, left, right])?;
                self.builder.ins().jump(merge_block, &[result, err]);

                self.builder.switch_to_block(merge_block);
                self.builder.seal_block(slow_path_block);
                self.builder.seal_block(merge_block);

                let result_val = self.builder.block_params(merge_block)[0];
                let error_val = self.builder.block_params(merge_block)[1];
                return Ok((result_val, error_val));
            }
        };

        // Check for overflow (result fits in small int range)
        // Small int range: -2^60 to 2^60-1 (61 bits of value)
        let min_small = self.builder.ins().iconst(types::I64, -(1i64 << 60));
        let max_small = self.builder.ins().iconst(types::I64, (1i64 << 60) - 1);
        let in_range_low = self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, result, min_small);
        let in_range_high = self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, result, max_small);
        let in_range = self.builder.ins().band(in_range_low, in_range_high);

        let overflow_block = self.builder.create_block();
        let tag_result_block = self.builder.create_block();
        self.builder.ins().brif(in_range, tag_result_block, &[], overflow_block, &[]);

        // Overflow - fall back to runtime for proper boxing
        self.builder.switch_to_block(overflow_block);
        self.builder.seal_block(overflow_block);
        // Box the result using runtime
        let boxed = self.call_runtime_single("rt_box_int", &[self.ctx_param, result]);
        let no_err = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge_block, &[boxed, no_err]);

        // Tag the result (shift left 3 and add tag)
        self.builder.switch_to_block(tag_result_block);
        self.builder.seal_block(tag_result_block);
        let shifted = self.builder.ins().ishl(result, three);
        let tagged = self.builder.ins().bor(shifted, small_int_tag);
        let no_err2 = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge_block, &[tagged, no_err2]);

        // Slow path: call runtime function
        self.builder.switch_to_block(slow_path_block);
        self.builder.seal_block(slow_path_block);
        let (slow_result, slow_err) = self.call_runtime(fallback, &[self.ctx_param, left, right])?;
        self.builder.ins().jump(merge_block, &[slow_result, slow_err]);

        // Merge block
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower an integer comparison with inline fast path.
    fn lower_int_cmp_inline(
        &mut self,
        call: &CallExpr,
        cmp: IntCmp,
        fallback: &'static str,
    ) -> Result<(Value, Value), CompileError> {
        if call.args.len() != 2 {
            return Err(CompileError::Internal(format!(
                "Comparison requires 2 arguments, got {}",
                call.args.len()
            )));
        }

        // Constant folding: if both operands are integer literals, compute at compile time
        if let (Some(left_lit), Some(right_lit)) = (
            try_extract_int_literal(&call.args[0].expr),
            try_extract_int_literal(&call.args[1].expr),
        ) {
            let result = match cmp {
                IntCmp::Eq => left_lit == right_lit,
                IntCmp::Ne => left_lit != right_lit,
                IntCmp::Lt => left_lit < right_lit,
                IntCmp::Le => left_lit <= right_lit,
                IntCmp::Gt => left_lit > right_lit,
                IntCmp::Ge => left_lit >= right_lit,
            };
            let boxed = self.builder.ins().iconst(types::I64, BoxedValue::bool(result).as_raw() as i64);
            let no_error = self.builder.ins().iconst(types::I64, 0);
            return Ok((boxed, no_error));
        }

        // Constant folding: if both operands are boolean literals (for Eq/Ne)
        if matches!(cmp, IntCmp::Eq | IntCmp::Ne) {
            if let (Some(left_lit), Some(right_lit)) = (
                try_extract_bool_literal(&call.args[0].expr),
                try_extract_bool_literal(&call.args[1].expr),
            ) {
                let result = match cmp {
                    IntCmp::Eq => left_lit == right_lit,
                    IntCmp::Ne => left_lit != right_lit,
                    _ => unreachable!(),
                };
                let boxed = self.builder.ins().iconst(types::I64, BoxedValue::bool(result).as_raw() as i64);
                let no_error = self.builder.ins().iconst(types::I64, 0);
                return Ok((boxed, no_error));
            }
        }

        // Lower both operands first
        let (left, left_err) = self.lower_expr(&call.args[0].expr)?;

        // Check for error in left
        let left_error_block = self.builder.create_block();
        let left_ok_block = self.builder.create_block();
        self.builder.ins().brif(left_err, left_error_block, &[], left_ok_block, &[]);

        // Left error path
        self.builder.switch_to_block(left_error_block);
        self.builder.seal_block(left_error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Left OK - evaluate right
        self.builder.switch_to_block(left_ok_block);
        self.builder.seal_block(left_ok_block);

        let (right, right_err) = self.lower_expr(&call.args[1].expr)?;

        // Check for error in right
        let right_error_block = self.builder.create_block();
        let check_tags_block = self.builder.create_block();
        self.builder.ins().brif(right_err, right_error_block, &[], check_tags_block, &[]);

        // Right error path
        self.builder.switch_to_block(right_error_block);
        self.builder.seal_block(right_error_block);
        let zero2 = self.builder.ins().iconst(types::I64, 0);
        let one2 = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero2, one2]);

        // Check if both are small integers (tagged)
        self.builder.switch_to_block(check_tags_block);
        self.builder.seal_block(check_tags_block);

        let tag_mask = self.builder.ins().iconst(types::I64, Self::TAG_MASK);
        let small_int_tag = self.builder.ins().iconst(types::I64, Self::TAG_SMALL_INT);

        let left_tag = self.builder.ins().band(left, tag_mask);
        let right_tag = self.builder.ins().band(right, tag_mask);

        let left_is_small = self.builder.ins().icmp(IntCC::Equal, left_tag, small_int_tag);
        let right_is_small = self.builder.ins().icmp(IntCC::Equal, right_tag, small_int_tag);
        let both_small = self.builder.ins().band(left_is_small, right_is_small);

        let fast_path_block = self.builder.create_block();
        let slow_path_block = self.builder.create_block();
        self.builder.ins().brif(both_small, fast_path_block, &[], slow_path_block, &[]);

        // Fast path: both are small integers - compare directly (tags are same, so comparison works)
        self.builder.switch_to_block(fast_path_block);
        self.builder.seal_block(fast_path_block);

        // For small ints, we can compare the raw tagged values directly for most operations
        // since the tag bits are the same. We need to sign-extend for signed comparisons.
        let three = self.builder.ins().iconst(types::I64, 3);
        let left_val = self.builder.ins().sshr(left, three);
        let right_val = self.builder.ins().sshr(right, three);

        let cmp_result = match cmp {
            IntCmp::Eq => self.builder.ins().icmp(IntCC::Equal, left_val, right_val),
            IntCmp::Ne => self.builder.ins().icmp(IntCC::NotEqual, left_val, right_val),
            IntCmp::Lt => self.builder.ins().icmp(IntCC::SignedLessThan, left_val, right_val),
            IntCmp::Le => self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, left_val, right_val),
            IntCmp::Gt => self.builder.ins().icmp(IntCC::SignedGreaterThan, left_val, right_val),
            IntCmp::Ge => self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, left_val, right_val),
        };

        // Convert bool to BoxedValue bool
        let true_val = self.builder.ins().iconst(types::I64, BoxedValue::bool(true).as_raw() as i64);
        let false_val = self.builder.ins().iconst(types::I64, BoxedValue::bool(false).as_raw() as i64);
        let result = self.builder.ins().select(cmp_result, true_val, false_val);
        let no_err = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge_block, &[result, no_err]);

        // Slow path: call runtime function
        self.builder.switch_to_block(slow_path_block);
        self.builder.seal_block(slow_path_block);
        let (slow_result, slow_err) = self.call_runtime(fallback, &[self.ctx_param, left, right])?;
        self.builder.ins().jump(merge_block, &[slow_result, slow_err]);

        // Merge block
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower integer negation with inline fast path.
    fn lower_int_neg_inline(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        let (operand, err) = if let Some(target) = &call.target {
            self.lower_expr(&target.expr)?
        } else if !call.args.is_empty() {
            self.lower_expr(&call.args[0].expr)?
        } else {
            return Err(CompileError::Internal("Negate requires operand".to_string()));
        };

        // Check for error
        let error_block = self.builder.create_block();
        let check_tag_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        self.builder.ins().brif(err, error_block, &[], check_tag_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Check if it's a small integer
        self.builder.switch_to_block(check_tag_block);
        self.builder.seal_block(check_tag_block);

        let tag_mask = self.builder.ins().iconst(types::I64, Self::TAG_MASK);
        let small_int_tag = self.builder.ins().iconst(types::I64, Self::TAG_SMALL_INT);
        let tag = self.builder.ins().band(operand, tag_mask);
        let is_small = self.builder.ins().icmp(IntCC::Equal, tag, small_int_tag);

        let fast_path_block = self.builder.create_block();
        let slow_path_block = self.builder.create_block();
        self.builder.ins().brif(is_small, fast_path_block, &[], slow_path_block, &[]);

        // Fast path: small integer
        self.builder.switch_to_block(fast_path_block);
        self.builder.seal_block(fast_path_block);

        // Extract value, negate, and re-tag
        let three = self.builder.ins().iconst(types::I64, 3);
        let val = self.builder.ins().sshr(operand, three);
        let neg_val = self.builder.ins().ineg(val);

        // Check for overflow (MIN_SMALL can't be negated)
        let min_small = self.builder.ins().iconst(types::I64, -(1i64 << 60));
        let is_min = self.builder.ins().icmp(IntCC::Equal, val, min_small);

        let overflow_block = self.builder.create_block();
        let tag_result_block = self.builder.create_block();
        self.builder.ins().brif(is_min, overflow_block, &[], tag_result_block, &[]);

        // Overflow - fall back to runtime
        self.builder.switch_to_block(overflow_block);
        self.builder.seal_block(overflow_block);
        let (overflow_result, overflow_err) = self.call_runtime("rt_neg", &[self.ctx_param, operand])?;
        self.builder.ins().jump(merge_block, &[overflow_result, overflow_err]);

        // Tag the result
        self.builder.switch_to_block(tag_result_block);
        self.builder.seal_block(tag_result_block);
        let shifted = self.builder.ins().ishl(neg_val, three);
        let tagged = self.builder.ins().bor(shifted, small_int_tag);
        let no_err = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge_block, &[tagged, no_err]);

        // Slow path: call runtime function
        self.builder.switch_to_block(slow_path_block);
        self.builder.seal_block(slow_path_block);
        let (slow_result, slow_err) = self.call_runtime("rt_neg", &[self.ctx_param, operand])?;
        self.builder.ins().jump(merge_block, &[slow_result, slow_err]);

        // Merge block
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower the size() builtin function.
    fn lower_builtin_size(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        // size() can be called as size(x) or x.size()
        let (operand, err) = if let Some(target) = &call.target {
            self.lower_expr(&target.expr)?
        } else if !call.args.is_empty() {
            self.lower_expr(&call.args[0].expr)?
        } else {
            return Err(CompileError::Internal("size() requires an argument".to_string()));
        };

        // Check for error
        let error_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        self.builder.ins().brif(err, error_block, &[], continue_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Continue path - call rt_size
        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);
        let (result, result_err) = self.call_runtime("rt_size", &[self.ctx_param, operand])?;
        self.builder.ins().jump(merge_block, &[result, result_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower the contains() builtin function.
    fn lower_builtin_contains(&mut self, call: &CallExpr) -> Result<(Value, Value), CompileError> {
        // contains() is called as str.contains(substr)
        let (target, target_err) = if let Some(t) = &call.target {
            self.lower_expr(&t.expr)?
        } else {
            return Err(CompileError::Internal("contains() requires a target".to_string()));
        };

        // Check target error
        let error_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        self.builder.ins().brif(target_err, error_block, &[], continue_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Continue - get the argument
        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);

        if call.args.is_empty() {
            return Err(CompileError::Internal("contains() requires a substring argument".to_string()));
        }

        let (substr, substr_err) = self.lower_expr(&call.args[0].expr)?;

        // Check substr error
        let error_block2 = self.builder.create_block();
        let call_block = self.builder.create_block();

        self.builder.ins().brif(substr_err, error_block2, &[], call_block, &[]);

        // Error path for substr
        self.builder.switch_to_block(error_block2);
        self.builder.seal_block(error_block2);
        let zero2 = self.builder.ins().iconst(types::I64, 0);
        let one2 = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero2, one2]);

        // Call rt_contains
        self.builder.switch_to_block(call_block);
        self.builder.seal_block(call_block);
        let (result, result_err) = self.call_runtime("rt_contains", &[self.ctx_param, target, substr])?;
        self.builder.ins().jump(merge_block, &[result, result_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower a unary built-in function (can be called as func(x) or x.func()).
    fn lower_builtin_unary(
        &mut self,
        call: &CallExpr,
        rt_func: &'static str,
    ) -> Result<(Value, Value), CompileError> {
        // Can be called as func(x) or x.func()
        let (operand, err) = if let Some(target) = &call.target {
            self.lower_expr(&target.expr)?
        } else if !call.args.is_empty() {
            self.lower_expr(&call.args[0].expr)?
        } else {
            return Err(CompileError::Internal(format!(
                "{}() requires an argument",
                call.func_name
            )));
        };

        // Check for error
        let error_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        self.builder.ins().brif(err, error_block, &[], continue_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Continue path - call runtime function
        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);
        let (result, result_err) = self.call_runtime(rt_func, &[self.ctx_param, operand])?;
        self.builder.ins().jump(merge_block, &[result, result_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower a varargs built-in function like max/min.
    fn lower_builtin_varargs(
        &mut self,
        call: &CallExpr,
        rt_func: &'static str,
    ) -> Result<(Value, Value), CompileError> {
        // Evaluate all arguments
        let mut arg_vals = Vec::with_capacity(call.args.len());
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        for arg in &call.args {
            let (val, err) = self.lower_expr(&arg.expr)?;

            // Check for error
            let error_block = self.builder.create_block();
            let continue_block = self.builder.create_block();

            self.builder.ins().brif(err, error_block, &[], continue_block, &[]);

            self.builder.switch_to_block(error_block);
            self.builder.seal_block(error_block);
            let zero = self.builder.ins().iconst(types::I64, 0);
            let one = self.builder.ins().iconst(types::I64, 1);
            self.builder.ins().jump(merge_block, &[zero, one]);

            self.builder.switch_to_block(continue_block);
            self.builder.seal_block(continue_block);

            arg_vals.push(val);
        }

        // Allocate stack space for arguments array
        let args_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (arg_vals.len().max(1) * 8) as u32,
            8,
        ));

        // Store arguments to stack
        for (i, val) in arg_vals.iter().enumerate() {
            let offset = (i * 8) as i32;
            self.builder.ins().stack_store(*val, args_slot, offset);
        }

        // Get pointer to args array
        let args_ptr = self.builder.ins().stack_addr(self.ptr_type, args_slot, 0);
        let args_len = self.builder.ins().iconst(types::I64, arg_vals.len() as i64);

        // Call the runtime function
        let (result, result_err) = self.call_runtime(rt_func, &[self.ctx_param, args_ptr, args_len])?;
        self.builder.ins().jump(merge_block, &[result, result_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Lower a method-style built-in function (target.method(arg)).
    fn lower_builtin_method(
        &mut self,
        call: &CallExpr,
        rt_func: &'static str,
    ) -> Result<(Value, Value), CompileError> {
        // Must be called as target.method(arg)
        let (target, target_err) = if let Some(t) = &call.target {
            self.lower_expr(&t.expr)?
        } else {
            return Err(CompileError::Internal(format!(
                "{}() requires a target",
                call.func_name
            )));
        };

        // Check target error
        let error_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, types::I64);
        self.builder.append_block_param(merge_block, types::I64);

        self.builder.ins().brif(target_err, error_block, &[], continue_block, &[]);

        // Error path
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero, one]);

        // Continue - get the argument
        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);

        if call.args.is_empty() {
            return Err(CompileError::Internal(format!(
                "{}() requires an argument",
                call.func_name
            )));
        }

        let (arg, arg_err) = self.lower_expr(&call.args[0].expr)?;

        // Check arg error
        let error_block2 = self.builder.create_block();
        let call_block = self.builder.create_block();

        self.builder.ins().brif(arg_err, error_block2, &[], call_block, &[]);

        // Error path for arg
        self.builder.switch_to_block(error_block2);
        self.builder.seal_block(error_block2);
        let zero2 = self.builder.ins().iconst(types::I64, 0);
        let one2 = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(merge_block, &[zero2, one2]);

        // Call runtime function
        self.builder.switch_to_block(call_block);
        self.builder.seal_block(call_block);
        let (result, result_err) = self.call_runtime(rt_func, &[self.ctx_param, target, arg])?;
        self.builder.ins().jump(merge_block, &[result, result_err]);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);

        let result_val = self.builder.block_params(merge_block)[0];
        let error_val = self.builder.block_params(merge_block)[1];

        Ok((result_val, error_val))
    }

    /// Call a runtime function that returns (value, error).
    fn call_runtime(
        &mut self,
        name: &'static str,
        args: &[Value],
    ) -> Result<(Value, Value), CompileError> {
        let func_id = self.runtime_funcs.get(name).ok_or_else(|| {
            CompileError::Internal(format!("Unknown runtime function: {}", name))
        })?;

        let func_ref = self.module.declare_func_in_func(*func_id, self.builder.func);
        let call = self.builder.ins().call(func_ref, args);
        let results = self.builder.inst_results(call);

        Ok((results[0], results[1]))
    }

    /// Call a runtime function that returns a single value.
    fn call_runtime_single(&mut self, name: &'static str, args: &[Value]) -> Value {
        let func_id = self.runtime_funcs.get(name).expect("Unknown runtime function");
        let func_ref = self.module.declare_func_in_func(*func_id, self.builder.func);
        let call = self.builder.ins().call(func_ref, args);
        self.builder.inst_results(call)[0]
    }

    /// Call a runtime function that returns nothing (void).
    fn call_runtime_void(&mut self, name: &'static str, args: &[Value]) {
        let func_id = self.runtime_funcs.get(name).expect("Unknown runtime function");
        let func_ref = self.module.declare_func_in_func(*func_id, self.builder.func);
        self.builder.ins().call(func_ref, args);
    }

    /// Create a string constant and return (pointer, length) values.
    fn string_constant(&mut self, s: &str) -> (Value, Value) {
        // Store the string in a Box that will be kept alive for the program's lifetime
        let boxed: Box<str> = s.into();
        let ptr = boxed.as_ptr() as i64;
        let len = boxed.len() as i64;
        self.data.string_constants.push(boxed);

        let ptr_val = self.builder.ins().iconst(self.ptr_type, ptr);
        let len_val = self.builder.ins().iconst(types::I64, len);

        (ptr_val, len_val)
    }
}
