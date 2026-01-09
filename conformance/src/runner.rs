//! Conformance test runner.
//!
//! This is a simplified version that works with the current cel crate.
//! Type checking is not available, and Struct/Enum types are not supported.
//! Many tests will fail due to missing features.

use cel::context::Context;
use cel::objects::Value as CelValue;
use cel::Program;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::proto::cel::expr::conformance::test::{
    simple_test::ResultMatcher, SimpleTest, SimpleTestFile,
};
use crate::textproto::parse_textproto_to_prost;
use crate::value_converter::proto_value_to_cel_value;

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
            for test in &section.test {
                // Filter by category if specified
                if let Some(ref filter_category) = self.category_filter {
                    if !test_name_matches_category(&test.name, filter_category) {
                        continue;
                    }
                }

                // Catch panics so we can continue running all tests
                let test_result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run_test(test)));

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

    fn run_test(&self, test: &SimpleTest) -> TestResult {
        let test_name = &test.name;

        // For check_only tests, fail since type checking is not available
        if test.check_only {
            return TestResult::Failed {
                name: test_name.clone(),
                error: "Type checking not available (check_only test)".to_string(),
            };
        }

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

        // Build context with bindings
        let mut context = Context::default();

        // Add container if specified (simplified - just note it)
        if !test.container.is_empty() {
            // Container support requires features not available in current cel crate
            // Tests using containers will likely fail
        }

        if !test.bindings.is_empty() {
            for (key, expr_value) in &test.bindings {
                // Extract Value from ExprValue
                let proto_value = match expr_value.kind.as_ref() {
                    Some(crate::proto::cel::expr::expr_value::Kind::Value(v)) => v,
                    _ => {
                        // Non-value bindings (errors/unknowns) cause test failure
                        return TestResult::Failed {
                            name: test_name.clone(),
                            error: format!("Binding '{}' is not a value (error/unknown)", key),
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
                            if values_equal(&actual_value, &expected_cel_value) {
                                TestResult::Passed {
                                    name: test_name.clone(),
                                }
                            } else {
                                TestResult::Failed {
                                    name: test_name.clone(),
                                    error: format!(
                                        "Expected {:?}, got {:?}",
                                        expected_cel_value, actual_value
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
            Some(ResultMatcher::Unknown(_)) => TestResult::Failed {
                name: test_name.clone(),
                error: "Unknown result matching not implemented".to_string(),
            },
            Some(ResultMatcher::AnyEvalErrors(_)) => TestResult::Failed {
                name: test_name.clone(),
                error: "Any eval errors matching not implemented".to_string(),
            },
            Some(ResultMatcher::AnyUnknowns(_)) => TestResult::Failed {
                name: test_name.clone(),
                error: "Any unknowns matching not implemented".to_string(),
            },
            Some(ResultMatcher::TypedResult(_typed_result)) => {
                // TypedResult requires type checking which is not available
                TestResult::Failed {
                    name: test_name.clone(),
                    error: "TypedResult matching requires type checker (not available)".to_string(),
                }
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
        (Timestamp(a), Timestamp(b)) => a == b,
        (Duration(a), Duration(b)) => a == b,
        _ => false,
    }
}

/// Check if a test name matches a category filter (before running the test).
fn test_name_matches_category(test_name: &str, category: &str) -> bool {
    let name_lower = test_name.to_lowercase();
    let category_lower = category.to_lowercase();

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
        _ => {
            // Try partial matching
            category_lower
                .split_whitespace()
                .any(|word| name_lower.contains(word))
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TestResults {
    pub passed: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl TestResults {
    pub fn merge(&mut self, other: TestResults) {
        self.passed.extend(other.passed);
        self.failed.extend(other.failed);
    }

    pub fn total(&self) -> usize {
        self.passed.len() + self.failed.len()
    }

    pub fn print_summary(&self) {
        let total = self.total();
        let passed = self.passed.len();
        let failed = self.failed.len();

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
        println!("  Total:   {}", total);

        if !self.failed.is_empty() {
            self.print_grouped_failures();
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
            // Show first 5 failures per category
            for failure in failures.iter().take(5) {
                println!("    - {}: {}", failure.0, failure.1);
            }
            if count > 5 {
                println!("    ... and {} more", count - 5);
            }
        }
    }
}

fn categorize_test(name: &str, error: &str) -> String {
    // Categorize by error type first
    if error.starts_with("Parse error:") {
        return "Parse errors".to_string();
    }

    if error.contains("Type checking not available") || error.contains("TypedResult") {
        return "Type checking (not available)".to_string();
    }

    if error.contains("not implemented") {
        return "Not implemented features".to_string();
    }

    if error.starts_with("Execution error:") {
        if error.contains("UndeclaredReference") {
            return "Undeclared references".to_string();
        }
        if error.contains("NoSuchKey") {
            return "Map key access errors".to_string();
        }
        if error.contains("NoSuchOverload") {
            return "Overload resolution errors".to_string();
        }
    }

    if error.contains("Failed to convert") {
        return "Value conversion errors".to_string();
    }

    // Categorize by test name patterns
    if name.contains("optional") || name.contains("opt") {
        return "Optional/Chaining operations".to_string();
    }
    if name.contains("struct") {
        return "Struct operations".to_string();
    }
    if name.contains("timestamp") || name.contains("Timestamp") {
        return "Timestamp operations".to_string();
    }
    if name.contains("duration") || name.contains("Duration") {
        return "Duration operations".to_string();
    }

    "Other failures".to_string()
}

#[derive(Debug)]
pub enum TestResult {
    Passed { name: String },
    Failed { name: String, error: String },
}

impl From<TestResult> for TestResults {
    fn from(result: TestResult) -> Self {
        match result {
            TestResult::Passed { name } => TestResults {
                passed: vec![name],
                failed: vec![],
            },
            TestResult::Failed { name, error } => TestResults {
                passed: vec![],
                failed: vec![(name, error)],
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
