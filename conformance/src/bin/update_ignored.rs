use std::collections::BTreeSet;
use std::io::{self, Read};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    if input.trim().is_empty() {
        return Err("No input provided on stdin. Pipe cargo test output into this command.".into());
    }

    let ignored_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("bin")
        .join("ignored.txt");
    let existing = std::fs::read_to_string(&ignored_path)?;

    let update = update_ignored(&existing, &input)?;

    for entry in &update.removed {
        eprintln!("Removed (now passing): {entry}");
    }
    for entry in &update.added {
        eprintln!("Added (now failing): {entry}");
    }

    if update.content == existing {
        eprintln!("No changes to {}", ignored_path.display());
        return Ok(());
    }

    std::fs::write(&ignored_path, &update.content)?;
    eprintln!(
        "Updated {}: {} added, {} removed",
        ignored_path.display(),
        update.added.len(),
        update.removed.len()
    );

    Ok(())
}

struct Update {
    content: String,
    added: Vec<String>,
    removed: Vec<String>,
}

/// Merges a `cargo test` log into the existing ignore list: plain failures are
/// appended, `should panic` tests that no longer panic are removed, and every
/// other line (including comments) is kept as is.
fn update_ignored(existing: &str, log: &str) -> Result<Update, String> {
    let (failures, unexpected_passes) = parse_test_log(log)?;

    let mut content = String::new();
    let mut present = BTreeSet::new();
    let mut removed = Vec::new();
    for line in existing.lines() {
        let entry = line.trim();
        if unexpected_passes.contains(entry) {
            removed.push(entry.to_string());
            continue;
        }
        present.insert(entry);
        content.push_str(line);
        content.push('\n');
    }

    let added: Vec<String> = failures
        .iter()
        .filter(|name| !unexpected_passes.contains(*name) && !present.contains(name.as_str()))
        .cloned()
        .collect();
    for entry in &added {
        content.push_str(entry);
        content.push('\n');
    }

    Ok(Update {
        content,
        added,
        removed,
    })
}

/// Returns every failing test name and the subset that failed because a
/// `should panic` test did not panic. Errors unless every test binary in the
/// log ran to its `test result:` line with a failure list matching its count.
fn parse_test_log(log: &str) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    let mut failures = BTreeSet::new();
    let mut unexpected_passes = BTreeSet::new();
    let mut running = 0;
    let mut results = 0;
    let mut segment_start = 0;
    let mut offset = 0;

    for line in log.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let line = line.trim_end();

        if line.starts_with("running ") && (line.ends_with(" tests") || line.ends_with(" test")) {
            running += 1;
        } else if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|l| l.strip_suffix(" - should panic ... FAILED"))
        {
            unexpected_passes.insert(name.to_string());
        } else if let Some(summary) = line.strip_prefix("test result: ") {
            results += 1;
            let failed = summary
                .split("; ")
                .find_map(|part| part.strip_suffix(" failed"))
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| format!("Unrecognised test summary: {line}"))?;

            let segment = &log[segment_start..line_start];
            let names: Vec<&str> = match segment.rfind("\nfailures:\n") {
                Some(index) => segment[index + "\nfailures:\n".len()..]
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .collect(),
                None => Vec::new(),
            };
            if names.len() != failed {
                return Err(format!(
                    "Failure list has {} entries but the summary reports {failed} failed: {line}",
                    names.len()
                ));
            }
            failures.extend(names.into_iter().map(String::from));
            segment_start = offset;
        }
    }

    if results == 0 || running != results {
        return Err("Incomplete test log: every `running N tests` needs a `test result:` line. Nothing was written.".into());
    }

    Ok((failures, unexpected_passes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXISTING: &str = "# comment kept\n\
                            a::b::still_broken\n\
                            a::b::fixed\n";

    fn log(body: &str, summary: &str) -> String {
        format!("\nrunning 3 tests\n{body}\ntest result: {summary}; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\n")
    }

    #[test]
    fn passing_run_is_a_no_op() {
        let log = log(
            "test a::b::still_broken - should panic ... ok\ntest a::b::fixed - should panic ... ok\n",
            "ok. 2 passed; 0 failed",
        );
        let update = update_ignored(EXISTING, &log).unwrap();
        assert_eq!(update.content, EXISTING);
        assert!(update.added.is_empty() && update.removed.is_empty());
    }

    #[test]
    fn new_failures_are_appended_sorted_and_old_entries_kept() {
        let log = log(
            "test a::z::new ... FAILED\ntest a::c::new ... FAILED\n\nfailures:\n\n---- a::z::new stdout ----\nboom\n\nfailures:\n    a::c::new\n    a::z::new\n",
            "FAILED. 1 passed; 2 failed",
        );
        let update = update_ignored(EXISTING, &log).unwrap();
        assert_eq!(update.content, format!("{EXISTING}a::c::new\na::z::new\n"));
        assert_eq!(update.added, ["a::c::new", "a::z::new"]);
    }

    #[test]
    fn unexpected_passes_are_removed() {
        let log = log(
            "test a::b::fixed - should panic ... FAILED\n\nfailures:\n\n---- a::b::fixed stdout ----\nnote: test did not panic as expected\n\nfailures:\n    a::b::fixed\n",
            "FAILED. 1 passed; 1 failed",
        );
        let update = update_ignored(EXISTING, &log).unwrap();
        assert_eq!(update.content, "# comment kept\na::b::still_broken\n");
        assert_eq!(update.removed, ["a::b::fixed"]);
        assert!(update.added.is_empty());
    }

    #[test]
    fn refresh_into_empty_list_adds_all_failures() {
        let log = log(
            "test a::b::x ... FAILED\n\nfailures:\n\n---- a::b::x stdout ----\nboom\n\nfailures:\n    a::b::x\n",
            "FAILED. 0 passed; 1 failed",
        );
        assert_eq!(update_ignored("", &log).unwrap().content, "a::b::x\n");

        // libtest says "running 1 test" for a single test.
        let single = log.replace("running 3 tests", "running 1 test");
        assert_eq!(update_ignored("", &single).unwrap().content, "a::b::x\n");
    }

    #[test]
    fn truncated_or_unrecognised_logs_are_rejected() {
        assert!(update_ignored(EXISTING, "garbage\n").is_err());
        assert!(update_ignored(EXISTING, "\nrunning 3 tests\ntest a::b::x ... FAILED\n").is_err());
        let miscounted = log("\nfailures:\n    a::b::x\n", "FAILED. 0 passed; 2 failed");
        assert!(update_ignored(EXISTING, &miscounted).is_err());
    }
}
