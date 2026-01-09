use cel::checker::TypeChecker;
use cel::context::Context;
use cel::objects::{Struct, Value as CelValue};
use cel::Program;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

use crate::proto::cel::expr::conformance::test::{
    simple_test::ResultMatcher, SimpleTest, SimpleTestFile,
};
use crate::textproto::parse_textproto_to_prost;
use crate::type_env::{build_type_env_from_decls, proto_type_to_cel_type, types_equal};
use crate::value_converter::proto_value_to_cel_value;

/// Get the integer value for an enum by its name.
///
/// This maps enum names like "BAZ" to their integer values (e.g., 2).
fn get_enum_value_by_name(type_name: &str, name: &str) -> Option<i64> {
    match type_name {
        "cel.expr.conformance.proto2.GlobalEnum" | "cel.expr.conformance.proto3.GlobalEnum" => {
            match name {
                "GOO" => Some(0),
                "GAR" => Some(1),
                "GAZ" => Some(2),
                _ => None,
            }
        }
        "cel.expr.conformance.proto2.TestAllTypes.NestedEnum"
        | "cel.expr.conformance.proto3.TestAllTypes.NestedEnum" => match name {
            "FOO" => Some(0),
            "BAR" => Some(1),
            "BAZ" => Some(2),
            _ => None,
        },
        "google.protobuf.NullValue" => match name {
            "NULL_VALUE" => Some(0),
            _ => None,
        },
        _ => None,
    }
}

/// Get a list of proto type names to register for a given container.
///
/// These types need to be available as variables so expressions like
/// `GlobalEnum.GAZ` can resolve `GlobalEnum` to the type name string.
fn get_container_type_names(container: &str) -> Vec<(String, String)> {
    let mut types = Vec::new();

    match container {
        "cel.expr.conformance.proto2" => {
            types.push((
                "cel.expr.conformance.proto2.TestAllTypes".to_string(),
                "cel.expr.conformance.proto2.TestAllTypes".to_string(),
            ));
            types.push((
                "cel.expr.conformance.proto2.NestedTestAllTypes".to_string(),
                "cel.expr.conformance.proto2.NestedTestAllTypes".to_string(),
            ));
            types.push((
                "cel.expr.conformance.proto2.GlobalEnum".to_string(),
                "cel.expr.conformance.proto2.GlobalEnum".to_string(),
            ));
            types.push((
                "cel.expr.conformance.proto2.TestAllTypes.NestedEnum".to_string(),
                "cel.expr.conformance.proto2.TestAllTypes.NestedEnum".to_string(),
            ));
        }
        "cel.expr.conformance.proto3" => {
            types.push((
                "cel.expr.conformance.proto3.TestAllTypes".to_string(),
                "cel.expr.conformance.proto3.TestAllTypes".to_string(),
            ));
            types.push((
                "cel.expr.conformance.proto3.NestedTestAllTypes".to_string(),
                "cel.expr.conformance.proto3.NestedTestAllTypes".to_string(),
            ));
            types.push((
                "cel.expr.conformance.proto3.GlobalEnum".to_string(),
                "cel.expr.conformance.proto3.GlobalEnum".to_string(),
            ));
            types.push((
                "cel.expr.conformance.proto3.TestAllTypes.NestedEnum".to_string(),
                "cel.expr.conformance.proto3.TestAllTypes.NestedEnum".to_string(),
            ));
        }
        "google.protobuf" => {
            types.push((
                "google.protobuf.NullValue".to_string(),
                "google.protobuf.NullValue".to_string(),
            ));
            types.push((
                "google.protobuf.Value".to_string(),
                "google.protobuf.Value".to_string(),
            ));
            types.push((
                "google.protobuf.ListValue".to_string(),
                "google.protobuf.ListValue".to_string(),
            ));
            types.push((
                "google.protobuf.Struct".to_string(),
                "google.protobuf.Struct".to_string(),
            ));
            // Wrapper types
            types.push((
                "google.protobuf.Int32Value".to_string(),
                "google.protobuf.Int32Value".to_string(),
            ));
            types.push((
                "google.protobuf.UInt32Value".to_string(),
                "google.protobuf.UInt32Value".to_string(),
            ));
            types.push((
                "google.protobuf.Int64Value".to_string(),
                "google.protobuf.Int64Value".to_string(),
            ));
            types.push((
                "google.protobuf.UInt64Value".to_string(),
                "google.protobuf.UInt64Value".to_string(),
            ));
            types.push((
                "google.protobuf.FloatValue".to_string(),
                "google.protobuf.FloatValue".to_string(),
            ));
            types.push((
                "google.protobuf.DoubleValue".to_string(),
                "google.protobuf.DoubleValue".to_string(),
            ));
            types.push((
                "google.protobuf.BoolValue".to_string(),
                "google.protobuf.BoolValue".to_string(),
            ));
            types.push((
                "google.protobuf.StringValue".to_string(),
                "google.protobuf.StringValue".to_string(),
            ));
            types.push((
                "google.protobuf.BytesValue".to_string(),
                "google.protobuf.BytesValue".to_string(),
            ));
        }
        _ => {}
    }

    types
}

pub struct ConformanceRunner {
    test_data_dir: PathBuf,
    category_filter: Option<String>,
}

impl ConformanceRunner {
    pub fn new(test_data_dir: PathBuf) -> Self {
        Self {
            test_data_dir,
            category_filter: None,
        }
    }

    pub fn with_category_filter(mut self, category: String) -> Self {
        self.category_filter = Some(category);
        self
    }

    pub fn run_all_tests(&self) -> Result<TestResults, RunnerError> {
        let mut results = TestResults::default();

        // Get the proto directory path
        let proto_dir = self
            .test_data_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("proto");

        // Walk through all .textproto files
        for entry in WalkDir::new(&self.test_data_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|s| s == "textproto")
                    .unwrap_or(false)
            })
        {
            let path = entry.path();
            let file_results = self.run_test_file(path, &proto_dir)?;
            results.merge(file_results);
        }

        Ok(results)
    }

    fn run_test_file(&self, path: &Path, proto_dir: &Path) -> Result<TestResults, RunnerError> {
        let content = fs::read_to_string(path)?;

        // Parse textproto using prost-reflect (with protoc fallback)
        let test_file: SimpleTestFile = parse_textproto_to_prost(
            &content,
            "cel.expr.conformance.test.SimpleTestFile",
            &["cel/expr/conformance/test/simple.proto"],
            &[proto_dir.to_str().unwrap()],
        )
        .map_err(|e| {
            RunnerError::ParseError(format!("Failed to parse {}: {}", path.display(), e))
        })?;

        let mut results = TestResults::default();

        // Run all tests in all sections
        for section in &test_file.section {
            // Detect legacy enum semantics based on section name
            let is_legacy_section = section.name.starts_with("legacy_");

            for test in &section.test {
                // Filter by category if specified
                if let Some(ref filter_category) = self.category_filter {
                    if !test_name_matches_category(&test.name, filter_category) {
                        continue;
                    }
                }

                // Catch panics so we can continue running all tests
                let test_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.run_test(test, is_legacy_section)
                }));

                let result = match test_result {
                    Ok(r) => r,
                    Err(_) => TestResult::Failed {
                        name: test.name.clone(),
                        error: "Test panicked during execution".to_string(),
                    },
                };
                results.merge(result.into());
            }
        }

        Ok(results)
    }

    fn run_test(&self, test: &SimpleTest, is_legacy: bool) -> TestResult {
        let test_name = &test.name;

        // Build type environment from test declarations
        let type_env = build_type_env_from_decls(
            &test.type_env,
            &if test.container.is_empty() {
                None
            } else {
                Some(test.container.clone())
            },
        );

        // Parse the expression - catch panics here too
        let program = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Program::compile(&test.expr)
        })) {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                return TestResult::Failed {
                    name: test_name.clone(),
                    error: format!("Parse error: {}", e),
                };
            }
            Err(_) => {
                return TestResult::Failed {
                    name: test_name.clone(),
                    error: "Panic during parsing".to_string(),
                };
            }
        };

        // Type check the expression
        let check_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut checker = TypeChecker::new(&type_env);
            checker.check_expr(program.expression())
        }));

        let checked_expr = match check_result {
            Ok(Ok(expr)) => Some(expr),
            Ok(Err(_)) => None, // Type checking failed, but we'll continue to evaluation
            Err(_) => None,     // Panic during type checking
        };

        // For check_only tests, only verify the type (don't evaluate)
        if test.check_only {
            return self.run_check_only_test(test_name, test, checked_expr.as_ref());
        }

        // Build context with bindings
        let mut context = Context::default().with_legacy_enum_semantics(is_legacy);

        // Always register fully qualified enum type maps for proto2 and proto3
        // This is needed for expressions like "cel.expr.conformance.proto2.TestAllTypes.NestedEnum.BAR"
        // even when no container is specified
        register_fully_qualified_enum_maps(&mut context, is_legacy);

        // Add container if specified
        if !test.container.is_empty() {
            context = context.with_container(test.container.clone());

            // Add proto type names and enum types for container-aware resolution
            for (type_name, _type_value) in get_container_type_names(&test.container) {
                // Register enum types as both functions and maps
                if type_name.contains("Enum") || type_name == "google.protobuf.NullValue" {
                    // Create factory function to generate enum constructors
                    let type_name_clone = type_name.clone();
                    let is_legacy_mode = is_legacy;
                    let create_enum_constructor = move |_ftx: &cel::FunctionContext,
                                                        value: cel::objects::Value|
                          -> Result<
                        cel::objects::Value,
                        cel::ExecutionError,
                    > {
                        match &value {
                            cel::objects::Value::String(name) => {
                                // Convert enum name to integer value with type info
                                let enum_value =
                                    get_enum_value_by_name(&type_name_clone, name.as_str())
                                        .ok_or_else(|| {
                                            cel::ExecutionError::function_error("enum", "invalid")
                                        })?;
                                // In legacy mode, return Int; in strong mode, return Enum
                                if is_legacy_mode {
                                    Ok(cel::objects::Value::Int(enum_value))
                                } else {
                                    Ok(cel::objects::Value::Enum(
                                        enum_value,
                                        Arc::new(type_name_clone.clone()),
                                    ))
                                }
                            }
                            cel::objects::Value::Int(i) => {
                                // Validate int32 range for enum values
                                if *i < i32::MIN as i64 || *i > i32::MAX as i64 {
                                    return Err(cel::ExecutionError::function_error(
                                        "enum", "range",
                                    ));
                                }
                                // In legacy mode, return Int; in strong mode, return Enum
                                if is_legacy_mode {
                                    Ok(cel::objects::Value::Int(*i))
                                } else {
                                    Ok(cel::objects::Value::Enum(
                                        *i,
                                        Arc::new(type_name_clone.clone()),
                                    ))
                                }
                            }
                            _ => {
                                // For other values, return as-is
                                Ok(value)
                            }
                        }
                    };

                    // Extract short name (e.g., "GlobalEnum" from "cel.expr.conformance.proto2.GlobalEnum")
                    if let Some(short_name) = type_name.rsplit('.').next() {
                        context.add_function(short_name, create_enum_constructor);
                    }

                    // For TestAllTypes.NestedEnum
                    if type_name.contains("TestAllTypes.NestedEnum") {
                        // Also register with parent prefix
                        let type_name_clone2 = type_name.clone();
                        let is_legacy_mode2 = is_legacy;
                        let create_enum_constructor2 = move |_ftx: &cel::FunctionContext,
                                                             value: cel::objects::Value|
                              -> Result<
                            cel::objects::Value,
                            cel::ExecutionError,
                        > {
                            match &value {
                                cel::objects::Value::String(name) => {
                                    let enum_value =
                                        get_enum_value_by_name(&type_name_clone2, name.as_str())
                                            .ok_or_else(|| {
                                                cel::ExecutionError::function_error(
                                                    "enum", "invalid",
                                                )
                                            })?;
                                    if is_legacy_mode2 {
                                        Ok(cel::objects::Value::Int(enum_value))
                                    } else {
                                        Ok(cel::objects::Value::Enum(
                                            enum_value,
                                            Arc::new(type_name_clone2.clone()),
                                        ))
                                    }
                                }
                                cel::objects::Value::Int(i) => {
                                    // Validate int32 range for enum values
                                    if *i < i32::MIN as i64 || *i > i32::MAX as i64 {
                                        return Err(cel::ExecutionError::function_error(
                                            "enum", "range",
                                        ));
                                    }
                                    if is_legacy_mode2 {
                                        Ok(cel::objects::Value::Int(*i))
                                    } else {
                                        Ok(cel::objects::Value::Enum(
                                            *i,
                                            Arc::new(type_name_clone2.clone()),
                                        ))
                                    }
                                }
                                _ => Ok(value),
                            }
                        };
                        context.add_function("TestAllTypes.NestedEnum", create_enum_constructor2);

                        // Also register TestAllTypes as a map with NestedEnum field
                        let nested_enum_type_name = type_name.clone();
                        let enum_val = |v: i64| -> cel::objects::Value {
                            if is_legacy {
                                cel::objects::Value::Int(v)
                            } else {
                                cel::objects::Value::Enum(
                                    v,
                                    Arc::new(nested_enum_type_name.clone()),
                                )
                            }
                        };
                        let mut nested_enum_map = std::collections::HashMap::new();
                        nested_enum_map.insert(
                            cel::objects::Key::String(Arc::new("FOO".to_string())),
                            enum_val(0),
                        );
                        nested_enum_map.insert(
                            cel::objects::Key::String(Arc::new("BAR".to_string())),
                            enum_val(1),
                        );
                        nested_enum_map.insert(
                            cel::objects::Key::String(Arc::new("BAZ".to_string())),
                            enum_val(2),
                        );

                        let mut test_all_types_fields = std::collections::HashMap::new();
                        test_all_types_fields.insert(
                            cel::objects::Key::String(Arc::new("NestedEnum".to_string())),
                            cel::objects::Value::Map(cel::objects::Map {
                                map: Arc::new(nested_enum_map),
                            }),
                        );

                        context.add_variable(
                            "TestAllTypes",
                            cel::objects::Value::Map(cel::objects::Map {
                                map: Arc::new(test_all_types_fields.clone()),
                            }),
                        );

                        // Also register with fully qualified path (e.g., "cel.expr.conformance.proto2.TestAllTypes")
                        // This is needed for expressions like "cel.expr.conformance.proto2.TestAllTypes.NestedEnum.BAR"
                        let qualified_test_all_types =
                            type_name.strip_suffix(".NestedEnum").unwrap_or(&type_name);
                        context.add_variable(
                            qualified_test_all_types,
                            cel::objects::Value::Map(cel::objects::Map {
                                map: Arc::new(test_all_types_fields),
                            }),
                        );
                    }

                    // For GlobalEnum - register as a map with enum values
                    if type_name.contains("GlobalEnum") && !type_name.contains("TestAllTypes") {
                        let global_enum_type_name = type_name.clone();
                        let enum_val = |v: i64| -> cel::objects::Value {
                            if is_legacy {
                                cel::objects::Value::Int(v)
                            } else {
                                cel::objects::Value::Enum(
                                    v,
                                    Arc::new(global_enum_type_name.clone()),
                                )
                            }
                        };
                        let mut global_enum_map = std::collections::HashMap::new();
                        global_enum_map.insert(
                            cel::objects::Key::String(Arc::new("GOO".to_string())),
                            enum_val(0),
                        );
                        global_enum_map.insert(
                            cel::objects::Key::String(Arc::new("GAR".to_string())),
                            enum_val(1),
                        );
                        global_enum_map.insert(
                            cel::objects::Key::String(Arc::new("GAZ".to_string())),
                            enum_val(2),
                        );

                        context.add_variable(
                            "GlobalEnum",
                            cel::objects::Value::Map(cel::objects::Map {
                                map: Arc::new(global_enum_map.clone()),
                            }),
                        );

                        // Also register with fully qualified path (e.g., "cel.expr.conformance.proto2.GlobalEnum")
                        context.add_variable(
                            &type_name,
                            cel::objects::Value::Map(cel::objects::Map {
                                map: Arc::new(global_enum_map),
                            }),
                        );
                    }

                    // For NullValue - register as a map with NULL_VALUE
                    if type_name == "google.protobuf.NullValue" {
                        let mut null_value_map = std::collections::HashMap::new();
                        null_value_map.insert(
                            cel::objects::Key::String(Arc::new("NULL_VALUE".to_string())),
                            cel::objects::Value::Int(0),
                        );

                        context.add_variable(
                            "NullValue",
                            cel::objects::Value::Map(cel::objects::Map {
                                map: Arc::new(null_value_map),
                            }),
                        );
                    }
                }
            }
        }

        if !test.bindings.is_empty() {
            for (key, expr_value) in &test.bindings {
                // Extract Value from ExprValue
                let proto_value = match expr_value.kind.as_ref() {
                    Some(crate::proto::cel::expr::expr_value::Kind::Value(v)) => v,
                    _ => {
                        return TestResult::Skipped {
                            name: test_name.clone(),
                            reason: format!("Binding '{}' is not a value (error/unknown)", key),
                        };
                    }
                };

                match proto_value_to_cel_value(proto_value) {
                    Ok(cel_value) => {
                        context.add_variable(key, cel_value);
                    }
                    Err(e) => {
                        return TestResult::Failed {
                            name: test_name.clone(),
                            error: format!("Failed to convert binding '{}': {}", key, e),
                        };
                    }
                }
            }
        }

        // Execute the program - catch panics
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| program.execute(&context)))
                .unwrap_or_else(|_| {
                    Err(cel::ExecutionError::function_error(
                        "execution",
                        "Panic during execution",
                    ))
                });

        // Check the result against the expected result
        match &test.result_matcher {
            Some(ResultMatcher::Value(expected_value)) => {
                match proto_value_to_cel_value(expected_value) {
                    Ok(expected_cel_value) => match result {
                        Ok(actual_value) => {
                            // Unwrap wrapper types before comparison
                            let actual_unwrapped = unwrap_wrapper_if_needed(actual_value.clone());
                            let expected_unwrapped =
                                unwrap_wrapper_if_needed(expected_cel_value.clone());

                            if values_equal(&actual_unwrapped, &expected_unwrapped) {
                                TestResult::Passed {
                                    name: test_name.clone(),
                                }
                            } else {
                                TestResult::Failed {
                                    name: test_name.clone(),
                                    error: format!(
                                        "Expected {:?}, got {:?}",
                                        expected_unwrapped, actual_unwrapped
                                    ),
                                }
                            }
                        }
                        Err(e) => TestResult::Failed {
                            name: test_name.clone(),
                            error: format!("Execution error: {:?}", e),
                        },
                    },
                    Err(e) => TestResult::Failed {
                        name: test_name.clone(),
                        error: format!("Failed to convert expected value: {}", e),
                    },
                }
            }
            Some(ResultMatcher::EvalError(_)) => {
                // Test expects an error
                match result {
                    Ok(_) => TestResult::Failed {
                        name: test_name.clone(),
                        error: "Expected error but got success".to_string(),
                    },
                    Err(_) => TestResult::Passed {
                        name: test_name.clone(),
                    },
                }
            }
            Some(ResultMatcher::Unknown(_)) => TestResult::Skipped {
                name: test_name.clone(),
                reason: "Unknown result matching not yet implemented".to_string(),
            },
            Some(ResultMatcher::AnyEvalErrors(_)) => TestResult::Skipped {
                name: test_name.clone(),
                reason: "Any eval errors matching not yet implemented".to_string(),
            },
            Some(ResultMatcher::AnyUnknowns(_)) => TestResult::Skipped {
                name: test_name.clone(),
                reason: "Any unknowns matching not yet implemented".to_string(),
            },
            Some(ResultMatcher::TypedResult(typed_result)) => {
                self.run_typed_result_test(test_name, result, typed_result, checked_expr.as_ref())
            }
            None => {
                // Default to expecting true
                match result {
                    Ok(CelValue::Bool(true)) => TestResult::Passed {
                        name: test_name.clone(),
                    },
                    Ok(v) => TestResult::Failed {
                        name: test_name.clone(),
                        error: format!("Expected true, got {:?}", v),
                    },
                    Err(e) => TestResult::Failed {
                        name: test_name.clone(),
                        error: format!("Execution error: {:?}", e),
                    },
                }
            }
        }
    }

    /// Handle check_only tests - verify type checking only, no evaluation
    fn run_check_only_test(
        &self,
        test_name: &str,
        test: &SimpleTest,
        checked_expr: Option<&cel::checker::CheckedExpr>,
    ) -> TestResult {
        // For check_only tests with typed_result, verify the deduced type
        if let Some(ResultMatcher::TypedResult(typed_result)) = &test.result_matcher {
            if let Some(expected_type) = &typed_result.deduced_type {
                match checked_expr {
                    Some(expr) => {
                        // Get the root expression type (ID 0 or the max ID)
                        let root_type = expr.get_root_type();

                        match root_type {
                            Some(actual_type) => {
                                if types_equal(expected_type, actual_type) {
                                    TestResult::Passed {
                                        name: test_name.to_string(),
                                    }
                                } else {
                                    let expected_cel_type = proto_type_to_cel_type(expected_type);
                                    TestResult::Failed {
                                        name: test_name.to_string(),
                                        error: format!(
                                            "Type mismatch: expected {:?}, got {:?}",
                                            expected_cel_type, actual_type
                                        ),
                                    }
                                }
                            }
                            None => TestResult::Failed {
                                name: test_name.to_string(),
                                error: "Type checking succeeded but no root type found".to_string(),
                            },
                        }
                    }
                    None => TestResult::Failed {
                        name: test_name.to_string(),
                        error: "Type checking failed".to_string(),
                    },
                }
            } else {
                // No expected type specified, just verify parsing and type checking succeeded
                if checked_expr.is_some() {
                    TestResult::Passed {
                        name: test_name.to_string(),
                    }
                } else {
                    TestResult::Failed {
                        name: test_name.to_string(),
                        error: "Type checking failed".to_string(),
                    }
                }
            }
        } else {
            // check_only without typed_result - just verify parsing and type checking work
            if checked_expr.is_some() {
                TestResult::Passed {
                    name: test_name.to_string(),
                }
            } else {
                TestResult::Failed {
                    name: test_name.to_string(),
                    error: "Type checking failed".to_string(),
                }
            }
        }
    }

    /// Handle TypedResult tests - verify both the value and deduced type
    fn run_typed_result_test(
        &self,
        test_name: &str,
        result: Result<CelValue, cel::ExecutionError>,
        typed_result: &crate::proto::cel::expr::conformance::test::TypedResult,
        checked_expr: Option<&cel::checker::CheckedExpr>,
    ) -> TestResult {
        // First, verify the deduced type if specified
        if let Some(expected_type) = &typed_result.deduced_type {
            match checked_expr {
                Some(expr) => {
                    let root_type = expr.get_root_type();
                    match root_type {
                        Some(actual_type) => {
                            if !types_equal(expected_type, actual_type) {
                                let expected_cel_type = proto_type_to_cel_type(expected_type);
                                return TestResult::Failed {
                                    name: test_name.to_string(),
                                    error: format!(
                                        "Type mismatch: expected {:?}, got {:?}",
                                        expected_cel_type, actual_type
                                    ),
                                };
                            }
                        }
                        None => {
                            return TestResult::Failed {
                                name: test_name.to_string(),
                                error: "Type checking succeeded but no root type found".to_string(),
                            };
                        }
                    }
                }
                None => {
                    return TestResult::Failed {
                        name: test_name.to_string(),
                        error: "Type checking failed".to_string(),
                    };
                }
            }
        }

        // Then, verify the result value if specified
        if let Some(expected_value) = &typed_result.result {
            match proto_value_to_cel_value(expected_value) {
                Ok(expected_cel_value) => match result {
                    Ok(actual_value) => {
                        let actual_unwrapped = unwrap_wrapper_if_needed(actual_value.clone());
                        let expected_unwrapped =
                            unwrap_wrapper_if_needed(expected_cel_value.clone());

                        if values_equal(&actual_unwrapped, &expected_unwrapped) {
                            TestResult::Passed {
                                name: test_name.to_string(),
                            }
                        } else {
                            TestResult::Failed {
                                name: test_name.to_string(),
                                error: format!(
                                    "Value mismatch: expected {:?}, got {:?}",
                                    expected_unwrapped, actual_unwrapped
                                ),
                            }
                        }
                    }
                    Err(e) => TestResult::Failed {
                        name: test_name.to_string(),
                        error: format!("Execution error: {:?}", e),
                    },
                },
                Err(e) => TestResult::Failed {
                    name: test_name.to_string(),
                    error: format!("Failed to convert expected value: {}", e),
                },
            }
        } else {
            // No expected value specified, just type checking passed
            TestResult::Passed {
                name: test_name.to_string(),
            }
        }
    }
}

fn values_equal(a: &CelValue, b: &CelValue) -> bool {
    use CelValue::*;
    match (a, b) {
        (Null, Null) => true,
        (Bool(a), Bool(b)) => a == b,
        (Int(a), Int(b)) => a == b,
        (UInt(a), UInt(b)) => a == b,
        (Float(a), Float(b)) => {
            // Handle NaN specially
            if a.is_nan() && b.is_nan() {
                true
            } else {
                a == b
            }
        }
        (String(a), String(b)) => a == b,
        (Bytes(a), Bytes(b)) => a == b,
        // Enum values compare by their integer value (type name is for type() introspection)
        (Enum(a, _), Enum(b, _)) => a == b,
        (Enum(a, _), Int(b)) => a == b,
        (Int(a), Enum(b, _)) => a == b,
        (List(a), List(b)) => {
            if a.len() != b.len() {
                return false;
            }
            a.iter().zip(b.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Map(a), Map(b)) => {
            if a.map.len() != b.map.len() {
                return false;
            }
            for (key, a_val) in a.map.iter() {
                match b.map.get(key) {
                    Some(b_val) => {
                        if !values_equal(a_val, b_val) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        }
        (Struct(a), Struct(b)) => structs_equal(a, b),
        (Timestamp(a), Timestamp(b)) => a == b,
        (Duration(a), Duration(b)) => a == b,
        _ => false,
    }
}

fn structs_equal(a: &Struct, b: &Struct) -> bool {
    // Special handling for google.protobuf.Any: compare semantically
    if a.type_name.as_str() == "google.protobuf.Any"
        && b.type_name.as_str() == "google.protobuf.Any"
    {
        return compare_any_structs(a, b);
    }

    // Type names must match
    if a.type_name != b.type_name {
        return false;
    }

    // Field counts must match
    if a.fields.len() != b.fields.len() {
        return false;
    }

    // All fields must have equal values
    for (key, value_a) in a.fields.iter() {
        match b.fields.get(key) {
            Some(value_b) => {
                if !values_equal(value_a, value_b) {
                    return false;
                }
            }
            None => return false,
        }
    }

    true
}

/// Compare two google.protobuf.Any structs semantically.
///
/// This function extracts the type_url and value fields from both structs
/// and performs semantic comparison of the protobuf wire format, so that
/// messages with the same content but different field order are considered equal.
fn compare_any_structs(a: &Struct, b: &Struct) -> bool {
    use cel::objects::Value as CelValue;

    // Extract type_url and value from both structs
    let type_url_a = a.fields.get("type_url");
    let type_url_b = b.fields.get("type_url");
    let value_a = a.fields.get("value");
    let value_b = b.fields.get("value");

    // Check type_url equality
    match (type_url_a, type_url_b) {
        (Some(CelValue::String(url_a)), Some(CelValue::String(url_b))) => {
            if url_a != url_b {
                return false; // Different message types
            }
        }
        (None, None) => {
            // Both missing type_url, fall back to bytewise comparison
            return match (value_a, value_b) {
                (Some(CelValue::Bytes(a)), Some(CelValue::Bytes(b))) => a == b,
                _ => false,
            };
        }
        _ => return false, // type_url mismatch
    }

    // Compare value bytes semantically
    match (value_a, value_b) {
        (Some(CelValue::Bytes(bytes_a)), Some(CelValue::Bytes(bytes_b))) => {
            cel::proto_compare::compare_any_values_semantic(bytes_a, bytes_b)
        }
        (None, None) => true, // Both empty
        _ => false,
    }
}

fn unwrap_wrapper_if_needed(value: CelValue) -> CelValue {
    match value {
        CelValue::Struct(s) => {
            // Check if this is a wrapper type
            let type_name = s.type_name.as_str();

            // Check if it's google.protobuf.Any and unpack it
            if type_name == "google.protobuf.Any" {
                // Extract type_url and value fields
                if let (Some(CelValue::String(type_url)), Some(CelValue::Bytes(value_bytes))) =
                    (s.fields.get("type_url"), s.fields.get("value"))
                {
                    // Create an Any message from the fields
                    use prost_types::Any;
                    let any = Any {
                        type_url: type_url.to_string(),
                        value: value_bytes.to_vec(),
                    };

                    // Try to unpack the Any to the actual type
                    if let Ok(unpacked) = crate::value_converter::convert_any_to_cel_value(&any) {
                        return unpacked;
                    }
                }

                // If unpacking fails, return the Any struct as-is
                return CelValue::Struct(s);
            }

            // Check if it's a Google protobuf wrapper type
            if !type_name.starts_with("google.protobuf.") || !type_name.ends_with("Value") {
                return CelValue::Struct(s);
            }

            // Check if the wrapper has a value field
            if let Some(v) = s.fields.get("value") {
                // Unwrap to the inner value
                return v.clone();
            }

            // Empty wrapper - return default value for the type
            match type_name {
                "google.protobuf.Int32Value" | "google.protobuf.Int64Value" => CelValue::Int(0),
                "google.protobuf.UInt32Value" | "google.protobuf.UInt64Value" => CelValue::UInt(0),
                "google.protobuf.FloatValue" | "google.protobuf.DoubleValue" => {
                    CelValue::Float(0.0)
                }
                "google.protobuf.StringValue" => CelValue::String(Arc::new(String::new())),
                "google.protobuf.BytesValue" => CelValue::Bytes(Arc::new(Vec::new())),
                "google.protobuf.BoolValue" => CelValue::Bool(false),
                _ => CelValue::Struct(s),
            }
        }
        other => other,
    }
}

#[derive(Debug, Default, Clone)]
pub struct TestResults {
    pub passed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub skipped: Vec<(String, String)>,
}

impl TestResults {
    pub fn merge(&mut self, other: TestResults) {
        self.passed.extend(other.passed);
        self.failed.extend(other.failed);
        self.skipped.extend(other.skipped);
    }

    pub fn total(&self) -> usize {
        self.passed.len() + self.failed.len() + self.skipped.len()
    }

    pub fn print_summary(&self) {
        let total = self.total();
        let passed = self.passed.len();
        let failed = self.failed.len();
        let skipped = self.skipped.len();

        println!("\nConformance Test Results:");
        println!(
            "  Passed:  {} ({:.1}%)",
            passed,
            if total > 0 {
                (passed as f64 / total as f64) * 100.0
            } else {
                0.0
            }
        );
        println!(
            "  Failed:  {} ({:.1}%)",
            failed,
            if total > 0 {
                (failed as f64 / total as f64) * 100.0
            } else {
                0.0
            }
        );
        println!(
            "  Skipped: {} ({:.1}%)",
            skipped,
            if total > 0 {
                (skipped as f64 / total as f64) * 100.0
            } else {
                0.0
            }
        );
        println!("  Total:   {}", total);

        if !self.failed.is_empty() {
            self.print_grouped_failures();
        }

        if !self.skipped.is_empty() && self.skipped.len() <= 20 {
            println!("\nSkipped tests:");
            for (name, reason) in &self.skipped {
                println!("  - {}: {}", name, reason);
            }
        } else if !self.skipped.is_empty() {
            println!(
                "\nSkipped {} tests (use --verbose to see details)",
                self.skipped.len()
            );
        }
    }

    fn print_grouped_failures(&self) {
        use std::collections::HashMap;

        // Group by test category based on test name patterns
        let mut category_groups: HashMap<String, Vec<&(String, String)>> = HashMap::new();

        for failure in &self.failed {
            let category = categorize_test(&failure.0, &failure.1);
            category_groups.entry(category).or_default().push(failure);
        }

        // Sort categories by count (descending)
        let mut categories: Vec<_> = category_groups.iter().collect();
        categories.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        println!("\nFailed tests by category:");
        for (category, failures) in &categories {
            let count = failures.len();
            let failure_word = if count == 1 { "failure" } else { "failures" };
            println!("\n  {} ({} {}):", category, count, failure_word);
            // Show all failures (no limit)
            for failure in failures.iter() {
                println!("    - {}: {}", failure.0, failure.1);
            }
        }
    }
}

fn categorize_test(name: &str, error: &str) -> String {
    // First, categorize by error type
    if error.starts_with("Parse error:") {
        if name.contains("optional") || name.contains("opt") {
            return "Optional/Chaining (Parse errors)".to_string();
        }
        return "Parse errors".to_string();
    }

    if error.starts_with("Execution error:") {
        // Categorize by error content
        if error.contains("UndeclaredReference") {
            let ref_name = extract_reference_name(error);
            if ref_name == "dyn" {
                return "Dynamic type operations".to_string();
            } else if ref_name == "format" {
                return "String formatting".to_string();
            } else if ref_name == "greatest" || ref_name == "least" {
                return "Math functions (greatest/least)".to_string();
            } else if ref_name == "exists" || ref_name == "all" || ref_name == "existsOne" {
                return "List/map operations (exists/all/existsOne)".to_string();
            } else if ref_name == "optMap" || ref_name == "optFlatMap" {
                return "Optional operations (optMap/optFlatMap)".to_string();
            } else if ref_name == "bind" {
                return "Macro/binding operations".to_string();
            } else if ref_name == "encode" || ref_name == "decode" {
                return "Encoding/decoding operations".to_string();
            } else if ref_name == "transformList" || ref_name == "transformMap" {
                return "Transform operations".to_string();
            } else if ref_name == "type" || ref_name == "google" {
                return "Type operations".to_string();
            } else if ref_name == "a" {
                return "Qualified identifier resolution".to_string();
            }
            return format!("Undeclared references ({})", ref_name);
        }

        if error.contains("FunctionError") && error.contains("Panic") {
            if name.contains("to_any") || name.contains("to_json") || name.contains("to_null") {
                return "Type conversions (to_any/to_json/to_null)".to_string();
            }
            if name.contains("eq_") || name.contains("ne_") {
                return "Equality operations (proto/type conversions)".to_string();
            }
            return "Function panics".to_string();
        }

        if error.contains("NoSuchKey") {
            return "Map key access errors".to_string();
        }

        if error.contains("UnsupportedBinaryOperator") {
            return "Binary operator errors".to_string();
        }

        if error.contains("ValuesNotComparable") {
            return "Comparison errors (bytes/unsupported)".to_string();
        }

        if error.contains("UnsupportedMapIndex") {
            return "Map index errors".to_string();
        }

        if error.contains("UnexpectedType") {
            return "Type mismatch errors".to_string();
        }

        if error.contains("DivisionByZero") {
            return "Division by zero errors".to_string();
        }

        if error.contains("NoSuchOverload") {
            return "Overload resolution errors".to_string();
        }
    }

    // Categorize by test name patterns
    if name.contains("optional") || name.contains("opt") {
        return "Optional/Chaining operations".to_string();
    }

    if name.contains("struct") {
        return "Struct operations".to_string();
    }

    if name.contains("string") || name.contains("String") {
        return "String operations".to_string();
    }

    if name.contains("format") {
        return "String formatting".to_string();
    }

    if name.contains("timestamp") || name.contains("Timestamp") {
        return "Timestamp operations".to_string();
    }

    if name.contains("duration") || name.contains("Duration") {
        return "Duration operations".to_string();
    }

    if name.contains("eq_") || name.contains("ne_") {
        return "Equality/inequality operations".to_string();
    }

    if name.contains("lt_")
        || name.contains("gt_")
        || name.contains("lte_")
        || name.contains("gte_")
    {
        return "Comparison operations (lt/gt/lte/gte)".to_string();
    }

    if name.contains("bytes") || name.contains("Bytes") {
        return "Bytes operations".to_string();
    }

    if name.contains("list") || name.contains("List") {
        return "List operations".to_string();
    }

    if name.contains("map") || name.contains("Map") {
        return "Map operations".to_string();
    }

    if name.contains("unicode") {
        return "Unicode operations".to_string();
    }

    if name.contains("conversion") || name.contains("Conversion") {
        return "Type conversions".to_string();
    }

    if name.contains("math") || name.contains("Math") {
        return "Math operations".to_string();
    }

    // Default category
    "Other failures".to_string()
}

fn extract_reference_name(error: &str) -> &str {
    // Extract the reference name from "UndeclaredReference(\"name\")"
    if let Some(start) = error.find("UndeclaredReference(\"") {
        let start = start + "UndeclaredReference(\"".len();
        if let Some(end) = error[start..].find('"') {
            return &error[start..start + end];
        }
    }
    "unknown"
}

/// Check if a test name matches a category filter (before running the test).
/// This is an approximation based on test name patterns.
fn test_name_matches_category(test_name: &str, category: &str) -> bool {
    let name_lower = test_name.to_lowercase();
    let category_lower = category.to_lowercase();

    // Match category names to test name patterns
    match category_lower.as_str() {
        "dynamic type operations" | "dynamic" => {
            name_lower.contains("dyn") || name_lower.contains("dynamic")
        }
        "string formatting" | "format" => {
            name_lower.contains("format") || name_lower.starts_with("format_")
        }
        "math functions (greatest/least)" | "greatest" | "least" | "math functions" => {
            name_lower.contains("greatest") || name_lower.contains("least")
        }
        "optional/chaining (parse errors)"
        | "optional/chaining operations"
        | "optional"
        | "chaining" => {
            name_lower.contains("optional")
                || name_lower.contains("opt")
                || name_lower.contains("chaining")
        }
        "struct operations" | "struct" => name_lower.contains("struct"),
        "string operations" | "string" => {
            name_lower.contains("string") && !name_lower.contains("format")
        }
        "timestamp operations" | "timestamp" => {
            name_lower.contains("timestamp") || name_lower.contains("time")
        }
        "duration operations" | "duration" => name_lower.contains("duration"),
        "equality/inequality operations" | "equality" | "inequality" => {
            name_lower.starts_with("eq_") || name_lower.starts_with("ne_")
        }
        "comparison operations (lt/gt/lte/gte)" | "comparison" => {
            name_lower.starts_with("lt_")
                || name_lower.starts_with("gt_")
                || name_lower.starts_with("lte_")
                || name_lower.starts_with("gte_")
        }
        "bytes operations" | "bytes" => name_lower.contains("bytes") || name_lower.contains("byte"),
        "list operations" | "list" => name_lower.contains("list") || name_lower.contains("elem"),
        "map operations" | "map" => name_lower.contains("map") && !name_lower.contains("optmap"),
        "unicode operations" | "unicode" => name_lower.contains("unicode"),
        "type conversions" | "conversion" => {
            name_lower.contains("conversion") || name_lower.starts_with("to_")
        }
        "parse errors" => {
            // We can't predict parse errors from the name, so include all tests
            // that might have parse errors (optional syntax, etc.)
            name_lower.contains("optional") || name_lower.contains("opt")
        }
        _ => {
            // Try partial matching
            category_lower
                .split_whitespace()
                .any(|word| name_lower.contains(word))
        }
    }
}

/// Register fully qualified enum type maps for proto2 and proto3
/// This enables expressions like "cel.expr.conformance.proto2.TestAllTypes.NestedEnum.BAR"
/// to resolve even without a container specified
fn register_fully_qualified_enum_maps(context: &mut cel::Context, is_legacy: bool) {
    // Helper to create enum value based on mode
    let enum_val = |value: i64, type_name: &str| -> cel::objects::Value {
        if is_legacy {
            cel::objects::Value::Int(value)
        } else {
            cel::objects::Value::Enum(value, Arc::new(type_name.to_string()))
        }
    };

    // Proto2 TestAllTypes.NestedEnum
    let proto2_nested_enum = "cel.expr.conformance.proto2.TestAllTypes.NestedEnum";
    let mut nested_enum_map = std::collections::HashMap::new();
    nested_enum_map.insert(
        cel::objects::Key::String(Arc::new("FOO".to_string())),
        enum_val(0, proto2_nested_enum),
    );
    nested_enum_map.insert(
        cel::objects::Key::String(Arc::new("BAR".to_string())),
        enum_val(1, proto2_nested_enum),
    );
    nested_enum_map.insert(
        cel::objects::Key::String(Arc::new("BAZ".to_string())),
        enum_val(2, proto2_nested_enum),
    );

    let mut test_all_types_fields = std::collections::HashMap::new();
    test_all_types_fields.insert(
        cel::objects::Key::String(Arc::new("NestedEnum".to_string())),
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(nested_enum_map.clone()),
        }),
    );

    // Register with fully qualified name
    context.add_variable(
        "cel.expr.conformance.proto2.TestAllTypes",
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(test_all_types_fields),
        }),
    );

    // Also register the NestedEnum directly
    context.add_variable(
        "cel.expr.conformance.proto2.TestAllTypes.NestedEnum",
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(nested_enum_map),
        }),
    );

    // Proto2 GlobalEnum
    let proto2_global_enum = "cel.expr.conformance.proto2.GlobalEnum";
    let mut global_enum_map = std::collections::HashMap::new();
    global_enum_map.insert(
        cel::objects::Key::String(Arc::new("GOO".to_string())),
        enum_val(0, proto2_global_enum),
    );
    global_enum_map.insert(
        cel::objects::Key::String(Arc::new("GAR".to_string())),
        enum_val(1, proto2_global_enum),
    );
    global_enum_map.insert(
        cel::objects::Key::String(Arc::new("GAZ".to_string())),
        enum_val(2, proto2_global_enum),
    );
    context.add_variable(
        "cel.expr.conformance.proto2.GlobalEnum",
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(global_enum_map),
        }),
    );

    // Proto3 TestAllTypes.NestedEnum
    let proto3_nested_enum = "cel.expr.conformance.proto3.TestAllTypes.NestedEnum";
    let mut nested_enum_map3 = std::collections::HashMap::new();
    nested_enum_map3.insert(
        cel::objects::Key::String(Arc::new("FOO".to_string())),
        enum_val(0, proto3_nested_enum),
    );
    nested_enum_map3.insert(
        cel::objects::Key::String(Arc::new("BAR".to_string())),
        enum_val(1, proto3_nested_enum),
    );
    nested_enum_map3.insert(
        cel::objects::Key::String(Arc::new("BAZ".to_string())),
        enum_val(2, proto3_nested_enum),
    );

    let mut test_all_types_fields3 = std::collections::HashMap::new();
    test_all_types_fields3.insert(
        cel::objects::Key::String(Arc::new("NestedEnum".to_string())),
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(nested_enum_map3.clone()),
        }),
    );

    context.add_variable(
        "cel.expr.conformance.proto3.TestAllTypes",
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(test_all_types_fields3),
        }),
    );

    context.add_variable(
        "cel.expr.conformance.proto3.TestAllTypes.NestedEnum",
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(nested_enum_map3),
        }),
    );

    // Proto3 GlobalEnum
    let proto3_global_enum = "cel.expr.conformance.proto3.GlobalEnum";
    let mut global_enum_map3 = std::collections::HashMap::new();
    global_enum_map3.insert(
        cel::objects::Key::String(Arc::new("GOO".to_string())),
        enum_val(0, proto3_global_enum),
    );
    global_enum_map3.insert(
        cel::objects::Key::String(Arc::new("GAR".to_string())),
        enum_val(1, proto3_global_enum),
    );
    global_enum_map3.insert(
        cel::objects::Key::String(Arc::new("GAZ".to_string())),
        enum_val(2, proto3_global_enum),
    );
    context.add_variable(
        "cel.expr.conformance.proto3.GlobalEnum",
        cel::objects::Value::Map(cel::objects::Map {
            map: Arc::new(global_enum_map3),
        }),
    );
}

#[derive(Debug)]
pub enum TestResult {
    Passed { name: String },
    Failed { name: String, error: String },
    Skipped { name: String, reason: String },
}

impl From<TestResult> for TestResults {
    fn from(result: TestResult) -> Self {
        match result {
            TestResult::Passed { name } => TestResults {
                passed: vec![name],
                failed: vec![],
                skipped: vec![],
            },
            TestResult::Failed { name, error } => TestResults {
                passed: vec![],
                failed: vec![(name, error)],
                skipped: vec![],
            },
            TestResult::Skipped { name, reason } => TestResults {
                passed: vec![],
                failed: vec![],
                skipped: vec![(name, reason)],
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Textproto parse error: {0}")]
    ParseError(String),
}
