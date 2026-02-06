use conformance::ConformanceRunner;
use std::panic;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut category_filter: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--category" | "-c" => {
                if i + 1 < args.len() {
                    category_filter = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: --category requires a category name");
                    eprintln!("\nUsage: {} [--category <category>]", args[0]);
                    eprintln!(
                        "\nExample: {} --category \"Dynamic type operations\"",
                        args[0]
                    );
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                println!("Usage: {} [OPTIONS]", args[0]);
                println!("\nOptions:");
                println!(
                    "  -c, --category <category>  Run only tests matching the specified category"
                );
                println!("  -h, --help                 Show this help message");
                println!("\nExamples:");
                println!("  {} --category \"Dynamic type operations\"", args[0]);
                println!("  {} --category \"String formatting\"", args[0]);
                println!("  {} --category \"Optional/Chaining operations\"", args[0]);
                std::process::exit(0);
            }
            arg => {
                eprintln!("Error: Unknown argument: {}", arg);
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }
    // Set a panic hook that suppresses the default panic output
    // We'll catch panics in the test runner and report them as failures
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        // Suppress panic output - we'll handle it in the test runner
        // Only show panics if RUST_BACKTRACE is set
        if std::env::var("RUST_BACKTRACE").is_ok() {
            default_hook(panic_info);
        }
    }));
    // Get the test data directory from the cel-spec submodule
    let test_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("cel-spec")
        .join("tests")
        .join("simple")
        .join("testdata");

    if !test_data_dir.exists() {
        eprintln!(
            "Error: Test data directory not found at: {}",
            test_data_dir.display()
        );
        eprintln!("Make sure the cel-spec submodule is initialized:");
        eprintln!("  git submodule update --init --recursive");
        std::process::exit(1);
    }

    if let Some(ref category) = category_filter {
        println!(
            "Running conformance tests from: {} (filtered by category: {})",
            test_data_dir.display(),
            category
        );
    } else {
        println!(
            "Running conformance tests from: {}",
            test_data_dir.display()
        );
    }

    let mut runner = ConformanceRunner::new(test_data_dir);
    if let Some(category) = category_filter {
        runner = runner.with_category_filter(category);
    }

    let results = match runner.run_all_tests() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error running tests: {}", e);
            std::process::exit(1);
        }
    };

    results.print_summary();

    // Exit with error code if there are failures, but still show all results
    if !results.failed.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}
