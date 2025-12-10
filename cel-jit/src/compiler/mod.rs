//! CEL expression compiler using Cranelift.
//!
//! This module compiles CEL expressions to native code using Cranelift's JIT.

pub mod lowering;
pub mod runtime;

use crate::error::CompileError;
use crate::runtime::{RuntimeContext, RuntimeResult};
use cel::{parser::Expression, Program};
use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context as CodegenContext;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use std::collections::HashMap;

/// Compiled CEL expression function signature.
/// Takes a RuntimeContext pointer and returns (value, error_flag).
pub type CompiledFn = unsafe extern "C" fn(*mut RuntimeContext) -> RuntimeResult;

/// The CEL expression compiler.
pub struct Compiler {
    /// The JIT module for compiling and linking.
    /// Wrapped in Option to allow taking ownership in Drop for explicit memory cleanup.
    module: Option<JITModule>,
    /// Function builder context (reusable).
    builder_ctx: FunctionBuilderContext,
    /// Cranelift codegen context.
    ctx: CodegenContext,
    /// Registered runtime function IDs.
    runtime_funcs: HashMap<&'static str, FuncId>,
}

impl Compiler {
    /// Get a reference to the JIT module.
    #[inline]
    fn module(&self) -> &JITModule {
        self.module.as_ref().expect("JITModule already taken")
    }

    /// Get a mutable reference to the JIT module.
    #[inline]
    fn module_mut(&mut self) -> &mut JITModule {
        self.module.as_mut().expect("JITModule already taken")
    }

    /// Create a new compiler instance.
    pub fn new() -> Result<Self, CompileError> {
        let mut flag_builder = settings::builder();
        flag_builder
            .set("use_colocated_libcalls", "false")
            .map_err(|e| CompileError::Cranelift(e.to_string()))?;
        flag_builder
            .set("is_pic", "false")
            .map_err(|e| CompileError::Cranelift(e.to_string()))?;
        flag_builder
            .set("opt_level", "speed")
            .map_err(|e| CompileError::Cranelift(e.to_string()))?;

        let isa_builder = cranelift_native::builder()
            .map_err(|e| CompileError::Cranelift(e.to_string()))?;

        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| CompileError::Cranelift(e.to_string()))?;

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

        // Register runtime functions
        runtime::register_runtime_symbols(&mut builder);

        let module = JITModule::new(builder);

        let mut compiler = Compiler {
            module: Some(module),
            builder_ctx: FunctionBuilderContext::new(),
            ctx: CodegenContext::new(),
            runtime_funcs: HashMap::new(),
        };

        // Declare runtime functions
        compiler.declare_runtime_functions()?;

        Ok(compiler)
    }

    /// Declare all runtime functions in the module.
    fn declare_runtime_functions(&mut self) -> Result<(), CompileError> {
        let ptr_type = self.module().target_config().pointer_type();

        // Binary operators: (ctx, left, right) -> (value, error)
        let binary_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // left
            sig.params.push(AbiParam::new(types::I64)); // right
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        // Unary operators: (ctx, val) -> (value, error)
        let unary_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // val
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        // to_bool: (ctx, val) -> bool (single i64)
        let to_bool_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // val
            sig.returns.push(AbiParam::new(types::I64)); // bool
            sig
        };

        // get_variable: (ctx, name_ptr, name_len) -> (value, error)
        let get_var_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(ptr_type)); // name_ptr
            sig.params.push(AbiParam::new(types::I64)); // name_len
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        // member: (ctx, target, field_ptr, field_len) -> (value, error)
        let member_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // target
            sig.params.push(AbiParam::new(ptr_type)); // field_ptr
            sig.params.push(AbiParam::new(types::I64)); // field_len
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        // index: (ctx, target, index) -> (value, error)
        let index_sig = binary_sig.clone();

        // make_list: (ctx, elements_ptr, len) -> (value, error)
        let make_list_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(ptr_type)); // elements_ptr
            sig.params.push(AbiParam::new(types::I64)); // len
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        // make_map: (ctx, keys_ptr, values_ptr, len) -> (value, error)
        let make_map_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(ptr_type)); // keys_ptr
            sig.params.push(AbiParam::new(ptr_type)); // values_ptr
            sig.params.push(AbiParam::new(types::I64)); // len
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        // box_int: (ctx, val) -> boxed
        let box_int_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // val
            sig.returns.push(AbiParam::new(types::I64)); // boxed
            sig
        };

        // box_float: (ctx, val) -> boxed
        let box_float_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::F64)); // val
            sig.returns.push(AbiParam::new(types::I64)); // boxed
            sig
        };

        // box_string: (ctx, ptr, len) -> boxed
        let box_string_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(ptr_type)); // ptr
            sig.params.push(AbiParam::new(types::I64)); // len
            sig.returns.push(AbiParam::new(types::I64)); // boxed
            sig
        };

        // Declare binary operators
        for name in &["rt_add", "rt_sub", "rt_mul", "rt_div", "rt_rem", "rt_eq", "rt_ne", "rt_lt", "rt_le", "rt_gt", "rt_ge", "rt_in"] {
            let id = self
                .module_mut()
                .declare_function(name, Linkage::Import, &binary_sig)
                .map_err(|e| CompileError::Module(e.to_string()))?;
            self.runtime_funcs.insert(name, id);
        }

        // Declare unary operators
        for name in &["rt_not", "rt_neg"] {
            let id = self
                .module_mut()
                .declare_function(name, Linkage::Import, &unary_sig)
                .map_err(|e| CompileError::Module(e.to_string()))?;
            self.runtime_funcs.insert(name, id);
        }

        // Declare rt_to_bool
        let id = self
            .module_mut()
            .declare_function("rt_to_bool", Linkage::Import, &to_bool_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_to_bool", id);

        // Declare rt_get_variable
        let id = self
            .module_mut()
            .declare_function("rt_get_variable", Linkage::Import, &get_var_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_get_variable", id);

        // Declare rt_member
        let id = self
            .module_mut()
            .declare_function("rt_member", Linkage::Import, &member_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_member", id);

        // Declare rt_index
        let id = self
            .module_mut()
            .declare_function("rt_index", Linkage::Import, &index_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_index", id);

        // Declare rt_make_list
        let id = self
            .module_mut()
            .declare_function("rt_make_list", Linkage::Import, &make_list_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_make_list", id);

        // Declare rt_make_map
        let id = self
            .module_mut()
            .declare_function("rt_make_map", Linkage::Import, &make_map_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_make_map", id);

        // Declare box functions
        let id = self
            .module_mut()
            .declare_function("rt_box_int", Linkage::Import, &box_int_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_box_int", id);

        let id = self
            .module_mut()
            .declare_function("rt_box_uint", Linkage::Import, &box_int_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_box_uint", id);

        let id = self
            .module_mut()
            .declare_function("rt_box_float", Linkage::Import, &box_float_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_box_float", id);

        let id = self
            .module_mut()
            .declare_function("rt_box_string", Linkage::Import, &box_string_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_box_string", id);

        let id = self
            .module_mut()
            .declare_function("rt_box_bytes", Linkage::Import, &box_string_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_box_bytes", id);

        // rt_size: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_size", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_size", id);

        // rt_contains: (ctx, target, arg) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_contains", Linkage::Import, &binary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_contains", id);

        // rt_starts_with: (ctx, target, prefix) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_starts_with", Linkage::Import, &binary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_starts_with", id);

        // rt_ends_with: (ctx, target, suffix) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_ends_with", Linkage::Import, &binary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_ends_with", id);

        // rt_string: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_string", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_string", id);

        // rt_int: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_int", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_int", id);

        // rt_uint: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_uint", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_uint", id);

        // rt_double: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_double", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_double", id);

        // rt_bytes: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_bytes", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_bytes", id);

        // rt_type: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_type", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_type", id);

        // rt_max/rt_min: (ctx, vals_ptr, vals_len) -> (value, error)
        let varargs_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(ptr_type)); // vals_ptr
            sig.params.push(AbiParam::new(types::I64)); // vals_len
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_max", Linkage::Import, &varargs_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_max", id);

        let id = self
            .module_mut()
            .declare_function("rt_min", Linkage::Import, &varargs_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_min", id);

        // List helpers for comprehension: rt_list_len, rt_list_get, rt_list_append
        // rt_list_len: (ctx, list) -> len (single i64)
        let list_len_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // list
            sig.returns.push(AbiParam::new(types::I64)); // len
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_list_len", Linkage::Import, &list_len_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_list_len", id);

        // rt_list_get: (ctx, list, index) -> elem (single i64)
        let list_get_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // list
            sig.params.push(AbiParam::new(types::I64)); // index
            sig.returns.push(AbiParam::new(types::I64)); // elem
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_list_get", Linkage::Import, &list_get_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_list_get", id);

        // rt_list_append: (ctx, list, elem) -> new_list (single i64)
        let list_append_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // list
            sig.params.push(AbiParam::new(types::I64)); // elem
            sig.returns.push(AbiParam::new(types::I64)); // new_list
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_list_append", Linkage::Import, &list_append_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_list_append", id);

        // rt_set_variable: (ctx, name_ptr, name_len, val) -> void
        let set_var_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(ptr_type)); // name_ptr
            sig.params.push(AbiParam::new(types::I64)); // name_len
            sig.params.push(AbiParam::new(types::I64)); // val
            // no returns - void function
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_set_variable", Linkage::Import, &set_var_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_set_variable", id);

        // rt_not_strictly_false: (ctx, val) -> (value, error)
        let id = self
            .module_mut()
            .declare_function("rt_not_strictly_false", Linkage::Import, &unary_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_not_strictly_false", id);

        // rt_has: (ctx, target, field_ptr, field_len) -> (value, error)
        // Same signature as rt_member
        let id = self
            .module_mut()
            .declare_function("rt_has", Linkage::Import, &member_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_has", id);

        // rt_call_function: (ctx, name_ptr, name_len, target, has_target, args_ptr, args_len) -> (value, error)
        let call_func_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(ptr_type)); // name_ptr
            sig.params.push(AbiParam::new(types::I64)); // name_len
            sig.params.push(AbiParam::new(types::I64)); // target
            sig.params.push(AbiParam::new(types::I64)); // has_target
            sig.params.push(AbiParam::new(ptr_type)); // args_ptr
            sig.params.push(AbiParam::new(types::I64)); // args_len
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig.returns.push(AbiParam::new(types::I64)); // error
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_call_function", Linkage::Import, &call_func_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_call_function", id);

        // rt_set_slot: (ctx, slot, value) -> void
        let set_slot_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // slot
            sig.params.push(AbiParam::new(types::I64)); // value
            // no returns - void function
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_set_slot", Linkage::Import, &set_slot_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_set_slot", id);

        // rt_get_slot: (ctx, slot) -> value (single i64)
        let get_slot_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // slot
            sig.returns.push(AbiParam::new(types::I64)); // value
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_get_slot", Linkage::Import, &get_slot_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_get_slot", id);

        // rt_get_slot_cloned: (ctx, slot) -> value (single i64, cloned)
        // Same signature as rt_get_slot but returns a cloned value
        let id = self
            .module_mut()
            .declare_function("rt_get_slot_cloned", Linkage::Import, &get_slot_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_get_slot_cloned", id);

        // rt_free_value: (ctx, value) -> void
        let free_value_sig = {
            let mut sig = self.module_mut().make_signature();
            sig.params.push(AbiParam::new(ptr_type)); // ctx
            sig.params.push(AbiParam::new(types::I64)); // value
            // no returns - void function
            sig
        };

        let id = self
            .module_mut()
            .declare_function("rt_free_value", Linkage::Import, &free_value_sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;
        self.runtime_funcs.insert("rt_free_value", id);

        Ok(())
    }

    /// Compile a CEL expression to a function.
    /// Returns the compiled function and the lowering data that must be kept alive.
    pub fn compile_expression(&mut self, expr: &Expression) -> Result<(CompiledFn, lowering::LoweringData), CompileError> {
        let ptr_type = self.module().target_config().pointer_type();

        // Create function signature: (ctx: *mut RuntimeContext) -> (value: i64, error: i64)
        let mut sig = self.module_mut().make_signature();
        sig.params.push(AbiParam::new(ptr_type)); // ctx
        sig.returns.push(AbiParam::new(types::I64)); // value
        sig.returns.push(AbiParam::new(types::I64)); // error

        // Declare the function
        let func_id = self
            .module_mut()
            .declare_function("cel_expr", Linkage::Local, &sig)
            .map_err(|e| CompileError::Module(e.to_string()))?;

        // Clear the context for the new function
        self.ctx.clear();
        self.ctx.func.signature = sig;

        // Lowering data must outlive the compiled function
        let mut lowering_data = lowering::LoweringData::new();

        // Build the function
        {
            let mut builder = FunctionBuilder::new(&mut self.ctx.func, &mut self.builder_ctx);
            let entry_block = builder.create_block();

            builder.append_block_params_for_function_params(entry_block);
            builder.switch_to_block(entry_block);
            builder.seal_block(entry_block);

            let ctx_param = builder.block_params(entry_block)[0];

            // Create the lowering context
            let module = self.module.as_mut().expect("JITModule already taken");
            let mut lowerer = lowering::ExprLowerer::new(
                &mut builder,
                module,
                &self.runtime_funcs,
                ctx_param,
                ptr_type,
                &mut lowering_data,
            );

            // Lower the expression
            let (value, error) = lowerer.lower_expr(&expr.expr)?;

            // Return the result
            builder.ins().return_(&[value, error]);
            builder.finalize();
        }

        // Compile the function
        {
            let module = self.module.as_mut().expect("JITModule already taken");
            module
                .define_function(func_id, &mut self.ctx)
                .map_err(|e| CompileError::Cranelift(e.to_string()))?;
        }

        {
            let module = self.module.as_mut().expect("JITModule already taken");
            module.clear_context(&mut self.ctx);
            module
                .finalize_definitions()
                .map_err(|e| CompileError::Module(e.to_string()))?;
        }

        // Get the function pointer
        let code_ptr = self.module().get_finalized_function(func_id);

        Ok((unsafe { std::mem::transmute::<*const u8, CompiledFn>(code_ptr) }, lowering_data))
    }

    /// Compile a CEL program.
    /// Returns the compiled function and the lowering data that must be kept alive.
    pub fn compile_program(&mut self, program: &Program) -> Result<(CompiledFn, lowering::LoweringData), CompileError> {
        self.compile_expression(program.expression())
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new().expect("Failed to create compiler")
    }
}

impl Drop for Compiler {
    fn drop(&mut self) {
        // JITModule does not automatically free memory on drop.
        // We need to take ownership and explicitly call free_memory().
        // This is safe because:
        // 1. The Compiler is being dropped, so no new function calls will be made
        // 2. CompiledProgram holds ownership of Compiler, ensuring the JIT memory
        //    remains valid while the compiled function might be called
        if let Some(module) = self.module.take() {
            // SAFETY: We're in Drop, so no code from this module will execute after this.
            unsafe {
                module.free_memory();
            }
        }
    }
}
