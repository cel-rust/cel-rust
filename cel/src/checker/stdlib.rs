//! Standard library function type signatures for CEL.
//!
//! This module registers type signatures for all standard CEL functions
//! including operators, type conversions, and built-in functions.

use super::env::{FunctionOverload, TypeEnv};
use super::types::CelType;

/// Register all standard CEL functions in the type environment.
pub fn register_standard_functions(env: &mut TypeEnv) {
    register_operators(env);
    register_type_conversions(env);
    register_string_functions(env);
    register_list_functions(env);
    register_map_functions(env);
    register_comparison_functions(env);
    register_math_functions(env);
    register_time_functions(env);
    register_type_functions(env);
    register_optional_functions(env);
}

fn register_operators(env: &mut TypeEnv) {
    // Arithmetic operators: _+_, _-_, _*_, _/_, _%_

    // Addition
    env.add_function(
        "_+_",
        FunctionOverload::new("add_int64", vec![CelType::Int, CelType::Int], CelType::Int),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_uint64",
            vec![CelType::Uint, CelType::Uint],
            CelType::Uint,
        ),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_double",
            vec![CelType::Double, CelType::Double],
            CelType::Double,
        ),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_string",
            vec![CelType::String, CelType::String],
            CelType::String,
        ),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_bytes",
            vec![CelType::Bytes, CelType::Bytes],
            CelType::Bytes,
        ),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_list",
            vec![
                CelType::List(Box::new(CelType::TypeParam("A".into()))),
                CelType::List(Box::new(CelType::TypeParam("A".into()))),
            ],
            CelType::List(Box::new(CelType::TypeParam("A".into()))),
        )
        .with_type_params(vec!["A"]),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_timestamp_duration",
            vec![CelType::Timestamp, CelType::Duration],
            CelType::Timestamp,
        ),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_duration_timestamp",
            vec![CelType::Duration, CelType::Timestamp],
            CelType::Timestamp,
        ),
    );
    env.add_function(
        "_+_",
        FunctionOverload::new(
            "add_duration_duration",
            vec![CelType::Duration, CelType::Duration],
            CelType::Duration,
        ),
    );

    // Subtraction
    env.add_function(
        "_-_",
        FunctionOverload::new(
            "subtract_int64",
            vec![CelType::Int, CelType::Int],
            CelType::Int,
        ),
    );
    env.add_function(
        "_-_",
        FunctionOverload::new(
            "subtract_uint64",
            vec![CelType::Uint, CelType::Uint],
            CelType::Uint,
        ),
    );
    env.add_function(
        "_-_",
        FunctionOverload::new(
            "subtract_double",
            vec![CelType::Double, CelType::Double],
            CelType::Double,
        ),
    );
    env.add_function(
        "_-_",
        FunctionOverload::new(
            "subtract_timestamp_timestamp",
            vec![CelType::Timestamp, CelType::Timestamp],
            CelType::Duration,
        ),
    );
    env.add_function(
        "_-_",
        FunctionOverload::new(
            "subtract_timestamp_duration",
            vec![CelType::Timestamp, CelType::Duration],
            CelType::Timestamp,
        ),
    );
    env.add_function(
        "_-_",
        FunctionOverload::new(
            "subtract_duration_duration",
            vec![CelType::Duration, CelType::Duration],
            CelType::Duration,
        ),
    );

    // Multiplication
    env.add_function(
        "_*_",
        FunctionOverload::new(
            "multiply_int64",
            vec![CelType::Int, CelType::Int],
            CelType::Int,
        ),
    );
    env.add_function(
        "_*_",
        FunctionOverload::new(
            "multiply_uint64",
            vec![CelType::Uint, CelType::Uint],
            CelType::Uint,
        ),
    );
    env.add_function(
        "_*_",
        FunctionOverload::new(
            "multiply_double",
            vec![CelType::Double, CelType::Double],
            CelType::Double,
        ),
    );

    // Division
    env.add_function(
        "_/_",
        FunctionOverload::new(
            "divide_int64",
            vec![CelType::Int, CelType::Int],
            CelType::Int,
        ),
    );
    env.add_function(
        "_/_",
        FunctionOverload::new(
            "divide_uint64",
            vec![CelType::Uint, CelType::Uint],
            CelType::Uint,
        ),
    );
    env.add_function(
        "_/_",
        FunctionOverload::new(
            "divide_double",
            vec![CelType::Double, CelType::Double],
            CelType::Double,
        ),
    );

    // Modulo
    env.add_function(
        "_%_",
        FunctionOverload::new(
            "modulo_int64",
            vec![CelType::Int, CelType::Int],
            CelType::Int,
        ),
    );
    env.add_function(
        "_%_",
        FunctionOverload::new(
            "modulo_uint64",
            vec![CelType::Uint, CelType::Uint],
            CelType::Uint,
        ),
    );

    // Negation
    env.add_function(
        "-_",
        FunctionOverload::new("negate_int64", vec![CelType::Int], CelType::Int),
    );
    env.add_function(
        "-_",
        FunctionOverload::new("negate_double", vec![CelType::Double], CelType::Double),
    );

    // Logical operators
    env.add_function(
        "!_",
        FunctionOverload::new("logical_not", vec![CelType::Bool], CelType::Bool),
    );
    env.add_function(
        "_&&_",
        FunctionOverload::new(
            "logical_and",
            vec![CelType::Bool, CelType::Bool],
            CelType::Bool,
        ),
    );
    env.add_function(
        "_||_",
        FunctionOverload::new(
            "logical_or",
            vec![CelType::Bool, CelType::Bool],
            CelType::Bool,
        ),
    );

    // Conditional operator
    env.add_function(
        "_?_:_",
        FunctionOverload::new(
            "conditional",
            vec![
                CelType::Bool,
                CelType::TypeParam("A".into()),
                CelType::TypeParam("A".into()),
            ],
            CelType::TypeParam("A".into()),
        )
        .with_type_params(vec!["A"]),
    );

    // Index operators
    env.add_function(
        "_[_]",
        FunctionOverload::new(
            "index_list",
            vec![
                CelType::List(Box::new(CelType::TypeParam("A".into()))),
                CelType::Int,
            ],
            CelType::TypeParam("A".into()),
        )
        .with_type_params(vec!["A"]),
    );
    env.add_function(
        "_[_]",
        FunctionOverload::new(
            "index_map",
            vec![
                CelType::Map(
                    Box::new(CelType::TypeParam("K".into())),
                    Box::new(CelType::TypeParam("V".into())),
                ),
                CelType::TypeParam("K".into()),
            ],
            CelType::TypeParam("V".into()),
        )
        .with_type_params(vec!["K", "V"]),
    );

    // In operator
    env.add_function(
        "@in",
        FunctionOverload::new(
            "in_list",
            vec![
                CelType::TypeParam("A".into()),
                CelType::List(Box::new(CelType::TypeParam("A".into()))),
            ],
            CelType::Bool,
        )
        .with_type_params(vec!["A"]),
    );
    env.add_function(
        "@in",
        FunctionOverload::new(
            "in_map",
            vec![
                CelType::TypeParam("K".into()),
                CelType::Map(
                    Box::new(CelType::TypeParam("K".into())),
                    Box::new(CelType::TypeParam("V".into())),
                ),
            ],
            CelType::Bool,
        )
        .with_type_params(vec!["K", "V"]),
    );
}

fn register_comparison_functions(env: &mut TypeEnv) {
    // Equality: _==_, _!=_
    let eq_types = [
        CelType::Bool,
        CelType::Int,
        CelType::Uint,
        CelType::Double,
        CelType::String,
        CelType::Bytes,
        CelType::Null,
        CelType::Timestamp,
        CelType::Duration,
    ];

    for typ in &eq_types {
        let type_name = format!("{}", typ);
        env.add_function(
            "_==_",
            FunctionOverload::new(
                format!("equals_{}", type_name),
                vec![typ.clone(), typ.clone()],
                CelType::Bool,
            ),
        );
        env.add_function(
            "_!=_",
            FunctionOverload::new(
                format!("not_equals_{}", type_name),
                vec![typ.clone(), typ.clone()],
                CelType::Bool,
            ),
        );
    }

    // Cross-numeric equality
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_int_uint",
            vec![CelType::Int, CelType::Uint],
            CelType::Bool,
        ),
    );
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_uint_int",
            vec![CelType::Uint, CelType::Int],
            CelType::Bool,
        ),
    );
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_int_double",
            vec![CelType::Int, CelType::Double],
            CelType::Bool,
        ),
    );
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_double_int",
            vec![CelType::Double, CelType::Int],
            CelType::Bool,
        ),
    );
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_uint_double",
            vec![CelType::Uint, CelType::Double],
            CelType::Bool,
        ),
    );
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_double_uint",
            vec![CelType::Double, CelType::Uint],
            CelType::Bool,
        ),
    );

    // List/map equality
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_list",
            vec![
                CelType::List(Box::new(CelType::TypeParam("A".into()))),
                CelType::List(Box::new(CelType::TypeParam("A".into()))),
            ],
            CelType::Bool,
        )
        .with_type_params(vec!["A"]),
    );
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_map",
            vec![
                CelType::Map(
                    Box::new(CelType::TypeParam("K".into())),
                    Box::new(CelType::TypeParam("V".into())),
                ),
                CelType::Map(
                    Box::new(CelType::TypeParam("K".into())),
                    Box::new(CelType::TypeParam("V".into())),
                ),
            ],
            CelType::Bool,
        )
        .with_type_params(vec!["K", "V"]),
    );

    // Generic equality for any type (including abstract types)
    // This handles cases like tuple<T,U,V> == tuple<T,U,V>
    env.add_function(
        "_==_",
        FunctionOverload::new(
            "equals_generic",
            vec![
                CelType::TypeParam("T".into()),
                CelType::TypeParam("T".into()),
            ],
            CelType::Bool,
        )
        .with_type_params(vec!["T"]),
    );
    env.add_function(
        "_!=_",
        FunctionOverload::new(
            "not_equals_generic",
            vec![
                CelType::TypeParam("T".into()),
                CelType::TypeParam("T".into()),
            ],
            CelType::Bool,
        )
        .with_type_params(vec!["T"]),
    );

    // Ordering: _<_, _<=_, _>=_, _>_
    let ord_types = [
        CelType::Int,
        CelType::Uint,
        CelType::Double,
        CelType::String,
        CelType::Bytes,
        CelType::Timestamp,
        CelType::Duration,
    ];

    for typ in &ord_types {
        let type_name = format!("{}", typ);
        for (op, op_name) in [
            ("_<_", "less"),
            ("_<=_", "less_equals"),
            ("_>=_", "greater_equals"),
            ("_>_", "greater"),
        ] {
            env.add_function(
                op,
                FunctionOverload::new(
                    format!("{}_{}", op_name, type_name),
                    vec![typ.clone(), typ.clone()],
                    CelType::Bool,
                ),
            );
        }
    }

    // Cross-numeric comparisons
    for (op, op_name) in [
        ("_<_", "less"),
        ("_<=_", "less_equals"),
        ("_>=_", "greater_equals"),
        ("_>_", "greater"),
    ] {
        env.add_function(
            op,
            FunctionOverload::new(
                format!("{}_int_uint", op_name),
                vec![CelType::Int, CelType::Uint],
                CelType::Bool,
            ),
        );
        env.add_function(
            op,
            FunctionOverload::new(
                format!("{}_uint_int", op_name),
                vec![CelType::Uint, CelType::Int],
                CelType::Bool,
            ),
        );
        env.add_function(
            op,
            FunctionOverload::new(
                format!("{}_int_double", op_name),
                vec![CelType::Int, CelType::Double],
                CelType::Bool,
            ),
        );
        env.add_function(
            op,
            FunctionOverload::new(
                format!("{}_double_int", op_name),
                vec![CelType::Double, CelType::Int],
                CelType::Bool,
            ),
        );
        env.add_function(
            op,
            FunctionOverload::new(
                format!("{}_uint_double", op_name),
                vec![CelType::Uint, CelType::Double],
                CelType::Bool,
            ),
        );
        env.add_function(
            op,
            FunctionOverload::new(
                format!("{}_double_uint", op_name),
                vec![CelType::Double, CelType::Uint],
                CelType::Bool,
            ),
        );
    }
}

fn register_type_conversions(env: &mut TypeEnv) {
    // int() conversions
    env.add_function(
        "int",
        FunctionOverload::new("int64", vec![CelType::Int], CelType::Int),
    );
    env.add_function(
        "int",
        FunctionOverload::new("uint64_to_int64", vec![CelType::Uint], CelType::Int),
    );
    env.add_function(
        "int",
        FunctionOverload::new("double_to_int64", vec![CelType::Double], CelType::Int),
    );
    env.add_function(
        "int",
        FunctionOverload::new("string_to_int64", vec![CelType::String], CelType::Int),
    );
    env.add_function(
        "int",
        FunctionOverload::new("timestamp_to_int64", vec![CelType::Timestamp], CelType::Int),
    );

    // uint() conversions
    env.add_function(
        "uint",
        FunctionOverload::new("uint64", vec![CelType::Uint], CelType::Uint),
    );
    env.add_function(
        "uint",
        FunctionOverload::new("int64_to_uint64", vec![CelType::Int], CelType::Uint),
    );
    env.add_function(
        "uint",
        FunctionOverload::new("double_to_uint64", vec![CelType::Double], CelType::Uint),
    );
    env.add_function(
        "uint",
        FunctionOverload::new("string_to_uint64", vec![CelType::String], CelType::Uint),
    );

    // double() conversions
    env.add_function(
        "double",
        FunctionOverload::new("double", vec![CelType::Double], CelType::Double),
    );
    env.add_function(
        "double",
        FunctionOverload::new("int64_to_double", vec![CelType::Int], CelType::Double),
    );
    env.add_function(
        "double",
        FunctionOverload::new("uint64_to_double", vec![CelType::Uint], CelType::Double),
    );
    env.add_function(
        "double",
        FunctionOverload::new("string_to_double", vec![CelType::String], CelType::Double),
    );

    // string() conversions
    env.add_function(
        "string",
        FunctionOverload::new("string", vec![CelType::String], CelType::String),
    );
    env.add_function(
        "string",
        FunctionOverload::new("int64_to_string", vec![CelType::Int], CelType::String),
    );
    env.add_function(
        "string",
        FunctionOverload::new("uint64_to_string", vec![CelType::Uint], CelType::String),
    );
    env.add_function(
        "string",
        FunctionOverload::new("double_to_string", vec![CelType::Double], CelType::String),
    );
    env.add_function(
        "string",
        FunctionOverload::new("bool_to_string", vec![CelType::Bool], CelType::String),
    );
    env.add_function(
        "string",
        FunctionOverload::new("bytes_to_string", vec![CelType::Bytes], CelType::String),
    );
    env.add_function(
        "string",
        FunctionOverload::new(
            "timestamp_to_string",
            vec![CelType::Timestamp],
            CelType::String,
        ),
    );
    env.add_function(
        "string",
        FunctionOverload::new(
            "duration_to_string",
            vec![CelType::Duration],
            CelType::String,
        ),
    );

    // bytes() conversions
    env.add_function(
        "bytes",
        FunctionOverload::new("bytes", vec![CelType::Bytes], CelType::Bytes),
    );
    env.add_function(
        "bytes",
        FunctionOverload::new("string_to_bytes", vec![CelType::String], CelType::Bytes),
    );

    // bool() - identity only (no string parsing in standard CEL)
    env.add_function(
        "bool",
        FunctionOverload::new("bool", vec![CelType::Bool], CelType::Bool),
    );

    // dyn() - marks value as dynamic
    env.add_function(
        "dyn",
        FunctionOverload::new("to_dyn", vec![CelType::TypeParam("A".into())], CelType::Dyn)
            .with_type_params(vec!["A"]),
    );

    // type() - returns the type of a value
    env.add_function(
        "type",
        FunctionOverload::new(
            "type",
            vec![CelType::TypeParam("A".into())],
            CelType::Type(Box::new(CelType::TypeParam("A".into()))),
        )
        .with_type_params(vec!["A"]),
    );

    // timestamp() and duration()
    env.add_function(
        "timestamp",
        FunctionOverload::new(
            "timestamp_string",
            vec![CelType::String],
            CelType::Timestamp,
        ),
    );
    env.add_function(
        "timestamp",
        FunctionOverload::new("timestamp_int", vec![CelType::Int], CelType::Timestamp),
    );
    env.add_function(
        "duration",
        FunctionOverload::new("duration_string", vec![CelType::String], CelType::Duration),
    );
}

fn register_string_functions(env: &mut TypeEnv) {
    // contains
    env.add_function(
        "contains",
        FunctionOverload::instance(
            "contains_string",
            CelType::String,
            vec![CelType::String],
            CelType::Bool,
        ),
    );

    // startsWith
    env.add_function(
        "startsWith",
        FunctionOverload::instance(
            "starts_with_string",
            CelType::String,
            vec![CelType::String],
            CelType::Bool,
        ),
    );

    // endsWith
    env.add_function(
        "endsWith",
        FunctionOverload::instance(
            "ends_with_string",
            CelType::String,
            vec![CelType::String],
            CelType::Bool,
        ),
    );

    // matches (regex)
    env.add_function(
        "matches",
        FunctionOverload::instance(
            "matches_string",
            CelType::String,
            vec![CelType::String],
            CelType::Bool,
        ),
    );
    env.add_function(
        "matches",
        FunctionOverload::new(
            "matches",
            vec![CelType::String, CelType::String],
            CelType::Bool,
        ),
    );

    // size
    env.add_function(
        "size",
        FunctionOverload::instance("size_string", CelType::String, vec![], CelType::Int),
    );
    env.add_function(
        "size",
        FunctionOverload::new("string_size", vec![CelType::String], CelType::Int),
    );
    env.add_function(
        "size",
        FunctionOverload::instance("size_bytes", CelType::Bytes, vec![], CelType::Int),
    );
    env.add_function(
        "size",
        FunctionOverload::new("bytes_size", vec![CelType::Bytes], CelType::Int),
    );
}

fn register_list_functions(env: &mut TypeEnv) {
    let list_a = CelType::List(Box::new(CelType::TypeParam("A".into())));

    // size
    env.add_function(
        "size",
        FunctionOverload::instance("size_list", list_a.clone(), vec![], CelType::Int)
            .with_type_params(vec!["A"]),
    );
    env.add_function(
        "size",
        FunctionOverload::new("list_size", vec![list_a.clone()], CelType::Int)
            .with_type_params(vec!["A"]),
    );

    // Comprehension functions (macros, but type checked as functions)
    // all, exists, exists_one, filter, map

    // all: list<A>.all(x, predicate) -> bool
    // These are macros and handled specially, but we register types for completeness

    // contains (for bytes)
    env.add_function(
        "contains",
        FunctionOverload::instance(
            "contains_bytes",
            CelType::Bytes,
            vec![CelType::Bytes],
            CelType::Bool,
        ),
    );
}

fn register_map_functions(env: &mut TypeEnv) {
    let map_kv = CelType::Map(
        Box::new(CelType::TypeParam("K".into())),
        Box::new(CelType::TypeParam("V".into())),
    );

    // size
    env.add_function(
        "size",
        FunctionOverload::instance("size_map", map_kv.clone(), vec![], CelType::Int)
            .with_type_params(vec!["K", "V"]),
    );
    env.add_function(
        "size",
        FunctionOverload::new("map_size", vec![map_kv.clone()], CelType::Int)
            .with_type_params(vec!["K", "V"]),
    );
}

fn register_math_functions(_env: &mut TypeEnv) {
    // These are typically extension functions but commonly used
}

fn register_time_functions(env: &mut TypeEnv) {
    // Timestamp accessor methods
    let timestamp_accessors = [
        ("getFullYear", "int"),
        ("getMonth", "int"),
        ("getDayOfYear", "int"),
        ("getDayOfMonth", "int"),
        ("getDate", "int"),
        ("getDayOfWeek", "int"),
        ("getHours", "int"),
        ("getMinutes", "int"),
        ("getSeconds", "int"),
        ("getMilliseconds", "int"),
    ];

    for (name, _ret) in timestamp_accessors {
        // Without timezone
        env.add_function(
            name,
            FunctionOverload::instance(
                format!("timestamp_{}", name),
                CelType::Timestamp,
                vec![],
                CelType::Int,
            ),
        );
        // With timezone string
        env.add_function(
            name,
            FunctionOverload::instance(
                format!("timestamp_{}_tz", name),
                CelType::Timestamp,
                vec![CelType::String],
                CelType::Int,
            ),
        );
    }

    // Duration accessor methods
    env.add_function(
        "getHours",
        FunctionOverload::instance("duration_getHours", CelType::Duration, vec![], CelType::Int),
    );
    env.add_function(
        "getMinutes",
        FunctionOverload::instance(
            "duration_getMinutes",
            CelType::Duration,
            vec![],
            CelType::Int,
        ),
    );
    env.add_function(
        "getSeconds",
        FunctionOverload::instance(
            "duration_getSeconds",
            CelType::Duration,
            vec![],
            CelType::Int,
        ),
    );
    env.add_function(
        "getMilliseconds",
        FunctionOverload::instance(
            "duration_getMilliseconds",
            CelType::Duration,
            vec![],
            CelType::Int,
        ),
    );
}

fn register_type_functions(_env: &mut TypeEnv) {
    // has() macro - checks field presence
    // This is a macro, but we can register a type for the field access pattern
}

use std::sync::Arc;

fn register_optional_functions(env: &mut TypeEnv) {
    // optional.none() -> optional_type<T>
    // Returns an empty optional that can hold any type
    env.add_function(
        "optional.none",
        FunctionOverload::new(
            "optional_none",
            vec![],
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
        )
        .with_type_params(vec![Arc::from("T")]),
    );

    // optional.of(T) -> optional_type<T>
    // Wraps a value in an optional
    env.add_function(
        "optional.of",
        FunctionOverload::new(
            "optional_of",
            vec![CelType::TypeParam(Arc::from("T"))],
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
        )
        .with_type_params(vec![Arc::from("T")]),
    );

    // optional.ofNonZeroValue(T) -> optional_type<T>
    // Wraps a value in an optional if it's not a zero value
    env.add_function(
        "optional.ofNonZeroValue",
        FunctionOverload::new(
            "optional_ofNonZeroValue",
            vec![CelType::TypeParam(Arc::from("T"))],
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
        )
        .with_type_params(vec![Arc::from("T")]),
    );

    // hasValue() -> bool (method on optional)
    env.add_function(
        "hasValue",
        FunctionOverload::instance(
            "optional_hasValue",
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
            vec![],
            CelType::Bool,
        )
        .with_type_params(vec![Arc::from("T")]),
    );

    // value() -> T (method on optional)
    env.add_function(
        "value",
        FunctionOverload::instance(
            "optional_value",
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
            vec![],
            CelType::TypeParam(Arc::from("T")),
        )
        .with_type_params(vec![Arc::from("T")]),
    );

    // or(optional_type<T>) -> optional_type<T> (method on optional)
    env.add_function(
        "or",
        FunctionOverload::instance(
            "optional_or_optional",
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
            vec![CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            )],
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
        )
        .with_type_params(vec![Arc::from("T")]),
    );

    // orValue(T) -> T (method on optional)
    env.add_function(
        "orValue",
        FunctionOverload::instance(
            "optional_orValue",
            CelType::Abstract(
                Arc::from("optional_type"),
                vec![CelType::TypeParam(Arc::from("T"))],
            ),
            vec![CelType::TypeParam(Arc::from("T"))],
            CelType::TypeParam(Arc::from("T")),
        )
        .with_type_params(vec![Arc::from("T")]),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_env() {
        let env = TypeEnv::standard();

        // Check some functions exist
        assert!(env.has_function("_+_"));
        assert!(env.has_function("_==_"));
        assert!(env.has_function("size"));
        assert!(env.has_function("contains"));
    }

    #[test]
    fn test_addition_overloads() {
        let env = TypeEnv::standard();

        // int + int -> int
        let matches = env.resolve_function("_+_", &[CelType::Int, CelType::Int], false);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].0.result_type, CelType::Int);

        // string + string -> string
        let matches = env.resolve_function("_+_", &[CelType::String, CelType::String], false);
        assert!(!matches.is_empty());
        assert_eq!(matches[0].0.result_type, CelType::String);
    }

    #[test]
    fn test_size_overloads() {
        let env = TypeEnv::standard();

        // size(string) -> int
        let matches = env.resolve_function("size", &[CelType::String], false);
        assert!(!matches.is_empty());

        // string.size() -> int
        let matches = env.resolve_function("size", &[CelType::String], true);
        assert!(!matches.is_empty());
    }
}
