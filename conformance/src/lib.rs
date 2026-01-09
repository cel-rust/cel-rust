pub mod proto;
pub mod runner;
pub mod textproto;
pub mod type_env;
pub mod value_converter;

pub use runner::{ConformanceRunner, TestResults};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn get_test_data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("cel-spec")
            .join("tests")
            .join("simple")
            .join("testdata")
    }

    fn run_conformance_tests(category: Option<&str>) -> TestResults {
        let test_data_dir = get_test_data_dir();

        if !test_data_dir.exists() {
            panic!(
                "Test data directory not found at: {}\n\
                Make sure the cel-spec submodule is initialized:\n\
                git submodule update --init --recursive",
                test_data_dir.display()
            );
        }

        let mut runner = ConformanceRunner::new(test_data_dir);
        if let Some(category) = category {
            runner = runner.with_category_filter(category.to_string());
        }

        runner
            .run_all_tests()
            .expect("Failed to run conformance tests")
    }

    #[test]
    fn conformance_all() {
        // Increase stack size to 8MB for prost-reflect parsing of complex nested messages
        let handle = std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let results = run_conformance_tests(None);
                results.print_summary();

                if !results.failed.is_empty() {
                    panic!(
                        "{} conformance test(s) failed. See output above for details.",
                        results.failed.len()
                    );
                }
            })
            .unwrap();

        // Propagate any panic from the thread
        if let Err(e) = handle.join() {
            std::panic::resume_unwind(e);
        }
    }

    // Category-specific tests - can be filtered with: cargo test conformance_dynamic
    #[test]
    fn conformance_dynamic() {
        let results = run_conformance_tests(Some("Dynamic type operations"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!(
                "{} dynamic type operation test(s) failed",
                results.failed.len()
            );
        }
    }

    #[test]
    fn conformance_string_formatting() {
        let results = run_conformance_tests(Some("String formatting"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} string formatting test(s) failed", results.failed.len());
        }
    }

    #[test]
    fn conformance_optional() {
        let results = run_conformance_tests(Some("Optional/Chaining operations"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} optional/chaining test(s) failed", results.failed.len());
        }
    }

    #[test]
    fn conformance_math_functions() {
        let results = run_conformance_tests(Some("Math functions (greatest/least)"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} math function test(s) failed", results.failed.len());
        }
    }

    #[test]
    fn conformance_struct() {
        let results = run_conformance_tests(Some("Struct operations"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} struct operation test(s) failed", results.failed.len());
        }
    }

    #[test]
    fn conformance_timestamp() {
        let results = run_conformance_tests(Some("Timestamp operations"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} timestamp test(s) failed", results.failed.len());
        }
    }

    #[test]
    fn conformance_duration() {
        let results = run_conformance_tests(Some("Duration operations"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} duration test(s) failed", results.failed.len());
        }
    }

    #[test]
    fn conformance_comparison() {
        let results = run_conformance_tests(Some("Comparison operations (lt/gt/lte/gte)"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} comparison test(s) failed", results.failed.len());
        }
    }

    #[test]
    fn conformance_equality() {
        let results = run_conformance_tests(Some("Equality/inequality operations"));
        results.print_summary();
        if !results.failed.is_empty() {
            panic!("{} equality test(s) failed", results.failed.len());
        }
    }
}
