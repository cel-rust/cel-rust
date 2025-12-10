//! Runtime function registration for the JIT compiler.

use crate::runtime::ops;
use cranelift_jit::JITBuilder;

/// Register all runtime symbols with the JIT builder.
pub fn register_runtime_symbols(builder: &mut JITBuilder) {
    // Binary operators
    builder.symbol("rt_add", ops::rt_add as *const u8);
    builder.symbol("rt_sub", ops::rt_sub as *const u8);
    builder.symbol("rt_mul", ops::rt_mul as *const u8);
    builder.symbol("rt_div", ops::rt_div as *const u8);
    builder.symbol("rt_rem", ops::rt_rem as *const u8);
    builder.symbol("rt_eq", ops::rt_eq as *const u8);
    builder.symbol("rt_ne", ops::rt_ne as *const u8);
    builder.symbol("rt_lt", ops::rt_lt as *const u8);
    builder.symbol("rt_le", ops::rt_le as *const u8);
    builder.symbol("rt_gt", ops::rt_gt as *const u8);
    builder.symbol("rt_ge", ops::rt_ge as *const u8);
    builder.symbol("rt_in", ops::rt_in as *const u8);

    // Unary operators
    builder.symbol("rt_not", ops::rt_not as *const u8);
    builder.symbol("rt_neg", ops::rt_neg as *const u8);

    // Utility functions
    builder.symbol("rt_to_bool", ops::rt_to_bool as *const u8);
    builder.symbol("rt_get_variable", ops::rt_get_variable as *const u8);
    builder.symbol("rt_member", ops::rt_member as *const u8);
    builder.symbol("rt_index", ops::rt_index as *const u8);

    // Collection constructors
    builder.symbol("rt_make_list", ops::rt_make_list as *const u8);
    builder.symbol("rt_make_map", ops::rt_make_map as *const u8);

    // Boxing functions
    builder.symbol("rt_box_int", ops::rt_box_int as *const u8);
    builder.symbol("rt_box_uint", ops::rt_box_uint as *const u8);
    builder.symbol("rt_box_float", ops::rt_box_float as *const u8);
    builder.symbol("rt_box_string", ops::rt_box_string as *const u8);
    builder.symbol("rt_box_bytes", ops::rt_box_bytes as *const u8);

    // Built-in functions
    builder.symbol("rt_size", ops::rt_size as *const u8);
    builder.symbol("rt_contains", ops::rt_contains as *const u8);
    builder.symbol("rt_starts_with", ops::rt_starts_with as *const u8);
    builder.symbol("rt_ends_with", ops::rt_ends_with as *const u8);
    builder.symbol("rt_string", ops::rt_string as *const u8);
    builder.symbol("rt_int", ops::rt_int as *const u8);
    builder.symbol("rt_uint", ops::rt_uint as *const u8);
    builder.symbol("rt_double", ops::rt_double as *const u8);
    builder.symbol("rt_bytes", ops::rt_bytes as *const u8);
    builder.symbol("rt_type", ops::rt_type as *const u8);
    builder.symbol("rt_max", ops::rt_max as *const u8);
    builder.symbol("rt_min", ops::rt_min as *const u8);

    // Comprehension helpers
    builder.symbol("rt_list_len", ops::rt_list_len as *const u8);
    builder.symbol("rt_list_get", ops::rt_list_get as *const u8);
    builder.symbol("rt_list_append", ops::rt_list_append as *const u8);
    builder.symbol("rt_set_variable", ops::rt_set_variable as *const u8);
    builder.symbol("rt_not_strictly_false", ops::rt_not_strictly_false as *const u8);
    builder.symbol("rt_has", ops::rt_has as *const u8);

    // Fast slot access for comprehension variables
    builder.symbol("rt_set_slot", ops::rt_set_slot as *const u8);
    builder.symbol("rt_get_slot", ops::rt_get_slot as *const u8);
    builder.symbol("rt_get_slot_cloned", ops::rt_get_slot_cloned as *const u8);

    // Memory management
    builder.symbol("rt_free_value", ops::rt_free_value as *const u8);

    builder.symbol("rt_call_function", ops::rt_call_function as *const u8);
}
