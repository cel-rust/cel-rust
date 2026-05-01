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

                // Tests are expected to fail - this is a baseline conformance suite
                // without the fixes that make tests pass
                if !results.failed.is_empty() {
                    panic!(
                        "{} conformance test(s) failed out of {} total. See output above for details.",
                        results.failed.len(),
                        results.total()
                    );
                }
            })
            .unwrap();

        // Propagate any panic from the thread
        if let Err(e) = handle.join() {
            std::panic::resume_unwind(e);
        }
    }
}
