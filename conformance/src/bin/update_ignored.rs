use std::io::{self, Read};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    if input.trim().is_empty() {
        return Err("No input provided on stdin. Pipe cargo test output into this command.".into());
    }

    let ignored = parse_ignored_from_test_log(&input);

    let ignored_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("bin")
        .join("ignored.txt");

    let mut out = String::new();
    for entry in &ignored {
        out.push_str(entry);
        out.push('\n');
    }

    std::fs::write(&ignored_path, out)?;
    eprintln!(
        "Wrote {} ignore entries to {}",
        ignored.len(),
        ignored_path.display()
    );

    Ok(())
}

fn parse_ignored_from_test_log(log: &str) -> Vec<String> {
    let mut ignored = Vec::new();

    let Some(result_index) = log.rfind("test result:") else {
        return ignored;
    };
    let Some(failures_index) = log[..result_index].rfind("failures:") else {
        return ignored;
    };

    let block_start = failures_index + "failures:".len();
    let block = log[block_start..result_index].trim();

    for entry in block.lines().map(str::trim).filter(|line| !line.is_empty()) {
        ignored.push(entry.to_string());
    }

    ignored
}
