use cargo_toml::Manifest;
use check_keyword::CheckKeyword;
use reqwest::blocking::Client;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

type IgnoredTests = HashMap<String, HashMap<String, HashSet<String>>>;
const IGNORED_TESTS_TEXT: &str = include_str!("ignored.txt");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = Manifest::from_path(manifest_dir.join("Cargo.toml"))?;
    let cel_spec_version = manifest
        .package()
        .metadata
        .as_ref()
        .ok_or("Missing [package.metadata] in Cargo.toml")?
        .get("generate")
        .ok_or("Missing [package.metadata.generate] in Cargo.toml")?
        .get("cel_spec_version")
        .ok_or("Missing cel_spec_version in [package.metadata.generate]")?
        .as_str()
        .ok_or("cel_spec_version must be a string")?;

    let workspace_root = manifest_dir
        .parent()
        .ok_or("Failed to locate workspace root")?
        .to_path_buf();
    let extracted_spec_root = fetch_cel_spec(
        &workspace_root.join("target").join("cel-spec-cache"),
        cel_spec_version,
    )?;
    let test_data_dir = extracted_spec_root
        .join("tests")
        .join("simple")
        .join("testdata");

    let gen_root = manifest_dir.join("src").join("gen");
    compile_protos(&extracted_spec_root.join("proto"), &gen_root)?;

    let tests_dir = manifest_dir.join("tests");
    let generated_tests_dir = tests_dir.join("gen");
    clear_directory(&generated_tests_dir)?;
    fs::create_dir_all(&tests_dir)?;
    let ignored_tests = parse_ignored_tests(IGNORED_TESTS_TEXT)?;
    let mut generated_modules = Vec::new();

    for path in find_files(&test_data_dir, "textproto") {
        let content = fs::read_to_string(&path)?;

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("Invalid test file name: {}", path.display()))?;
        let module_file = generated_tests_dir.join(format!("{}.rs", stem));
        generated_modules.push(stem.to_string());

        let file_content = render_file_module(stem, &content, &ignored_tests);
        fs::write(&module_file, file_content)?;
    }

    generated_modules.sort();
    fs::write(
        tests_dir.join("conformance.rs"),
        render_test_index(&generated_modules),
    )?;

    fs::write(
        gen_root.join("version.rs"),
        render_version_file(cel_spec_version),
    )?;

    println!(
        "Generated conformance tests in {}",
        generated_tests_dir.display()
    );

    Ok(())
}

fn compile_protos(proto_root: &Path, gen_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    clear_directory(gen_root)?;
    let paths = find_files(proto_root, "proto");
    if paths.is_empty() {
        return Err(format!("No .proto files found under {}", proto_root.display()).into());
    }

    let descriptor_set = protox::compile(&paths, &[proto_root.to_path_buf()])?;
    let descriptor_bytes = prost::Message::encode_to_vec(&descriptor_set);
    fs::write(gen_root.join("file_descriptor_set.bin"), descriptor_bytes)?;

    let mut prost_config = prost_build::Config::new();
    prost_config.out_dir(gen_root);
    prost_config.bytes(["."]);
    prost_config.compile_fds(descriptor_set)?;

    Ok(())
}

fn find_files(test_data_dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = WalkDir::new(test_data_dir)
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
                .map(|ext| ext == extension)
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    paths
}

struct TestCase {
    name: String,
    identifier: String,
    ignored: bool,
    source_lines: Vec<String>,
}

struct Section {
    name: String,
    description: Option<String>,
    identifier: String,
    tests: Vec<TestCase>,
}

struct ParsedFile {
    source_name: Option<String>,
    description: Option<String>,
    sections: Vec<Section>,
}

// Do a simple string-based parse of the textproto. The main reason to avoid textproto
// is it stores maps in a HashMap so generation cannot be deterministic. The format isn't
// difficult for the simple.proto schema.
fn parse_file_module(
    file_name: &str,
    test_textproto: &str,
    ignored_tests: &IgnoredTests,
) -> ParsedFile {
    let mut source_name = None;
    let mut description = None;
    let mut sections: Vec<Section> = Vec::new();
    let mut identifier_counts: HashMap<String, usize> = HashMap::new();
    let mut in_test = false;

    for line in test_textproto.lines() {
        if in_test {
            if line.starts_with("  }") {
                in_test = false;
            } else {
                sections
                    .last_mut()
                    .unwrap()
                    .tests
                    .last_mut()
                    .unwrap()
                    .source_lines
                    .push(line.to_string());
            }
            continue;
        }
        if line.starts_with("name: \"") {
            source_name = parse_quoted_field(line, "name: \"").map(str::to_string);
        } else if line.starts_with("description: \"") {
            description = parse_quoted_field(line, "description: \"").map(str::to_string);
        } else if line.starts_with("  name: \"") {
            if let Some(section_name) = parse_quoted_field(line, "  name: \"") {
                let identifier = sanitize_identifier(section_name);
                identifier_counts.clear();
                sections.push(Section {
                    name: section_name.to_string(),
                    description: None,
                    identifier,
                    tests: Vec::new(),
                });
            }
        } else if line.starts_with("  description: \"") {
            if let Some(desc) = parse_quoted_field(line, "  description: \"") {
                if let Some(section) = sections.last_mut() {
                    section.description = Some(desc.to_string());
                }
            }
        } else if line.starts_with("    name: \"") {
            if let Some(test_name) = parse_quoted_field(line, "    name: \"") {
                let identifier =
                    dedupe_identifier(sanitize_identifier(test_name), &mut identifier_counts);
                let section = sections.last_mut().unwrap();
                let ignored = should_ignore_test(
                    ignored_tests,
                    file_name,
                    Some(&section.identifier),
                    &identifier,
                );
                section.tests.push(TestCase {
                    name: test_name.to_string(),
                    identifier,
                    ignored,
                    source_lines: Vec::new(),
                });
                in_test = true;
            }
        }
    }

    ParsedFile {
        source_name,
        description,
        sections,
    }
}

fn render_file_module(
    file_name: &str,
    test_textproto: &str,
    ignored_tests: &IgnoredTests,
) -> String {
    let parsed = parse_file_module(file_name, test_textproto, ignored_tests);

    let mut out = String::new();
    out.push_str("// @generated by `cargo run -p conformance --bin generate`.\n");
    out.push_str("// DO NOT EDIT MANUALLY.\n\n");

    if let Some(ref name) = parsed.source_name {
        out.push_str(&format!("// Source file: {}\n", name));
    }
    if let Some(ref desc) = parsed.description {
        out.push_str(&format!("// Description: {}\n", desc));
    }

    for section in &parsed.sections {
        out.push('\n');
        out.push_str(&format!("// Section: {}\n", section.name));
        if let Some(ref desc) = section.description {
            out.push_str(&format!("// {}\n", desc));
        }
        out.push_str(&format!("mod {} {{\n", section.identifier));
        out.push_str("    use conformance::runner::run_test;\n");
        out.push_str("    use dedent::dedent;\n\n");

        for test in &section.tests {
            out.push_str(&format!("    // Test: {}\n", test.name));
            if test.ignored {
                out.push_str("    #[ignore]\n");
            }
            out.push_str("    #[test]\n");
            out.push_str(&format!("    fn {}() {{\n", test.identifier));
            out.push_str("        run_test(&dedent!(\n");
            out.push_str("            r#\"\n");
            for source_line in &test.source_lines {
                out.push_str(&format!("            {}\n", source_line));
            }
            out.push_str("            \"#\n");
            out.push_str("        ));\n");
            out.push_str("    }\n\n");
        }

        out.push_str("}\n\n");
    }

    out
}

fn parse_quoted_field<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?;
    let end_quote = rest.rfind('"')?;
    Some(&rest[..end_quote])
}

fn render_test_index(modules: &[String]) -> String {
    let mut out = String::new();
    out.push_str("// @generated by `cargo run -p conformance --bin generate`.\n");
    out.push_str("// DO NOT EDIT MANUALLY.\n\n");

    for module in modules {
        out.push_str(&format!("#[path = \"gen/{}.rs\"]\n", module));
        out.push_str(&format!("mod {};\n", module));
    }

    out
}

fn should_ignore_test(
    ignored_tests: &IgnoredTests,
    file_name: &str,
    section_name: Option<&str>,
    test_name: &str,
) -> bool {
    let Some(section_name) = section_name else {
        return false;
    };

    let Some(sections) = ignored_tests.get(file_name) else {
        return false;
    };

    let Some(tests) = sections.get(section_name) else {
        return false;
    };

    tests.contains(test_name)
}

fn parse_ignored_tests(input: &str) -> Result<IgnoredTests, Box<dyn std::error::Error>> {
    let mut ignored_tests: IgnoredTests = HashMap::new();

    for (line_number, line) in input.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.splitn(3, "::");
        let stem = parts.next().unwrap_or_default();
        let section_name = parts.next().unwrap_or_default();
        let test_name = parts.next().unwrap_or_default();

        if stem.is_empty() || section_name.is_empty() || test_name.is_empty() {
            return Err(format!(
                "Invalid ignore entry at line {}: {} (expected stem::section_name::test_name)",
                line_number + 1,
                trimmed
            )
            .into());
        }

        ignored_tests
            .entry(stem.to_string())
            .or_default()
            .entry(section_name.to_string())
            .or_default()
            .insert(test_name.to_string());
    }

    Ok(ignored_tests)
}

fn render_version_file(cel_spec_version: &str) -> String {
    let mut out = String::new();
    out.push_str("// @generated by `cargo run -p conformance --bin generate`.\n");
    out.push_str("// DO NOT EDIT MANUALLY.\n\n");
    out.push_str(&format!("// CEL_SPEC_VERSION: {}\n\n", cel_spec_version));
    out.push_str(&format!(
        "pub const CEL_SPEC_VERSION: &str = \"{}\";\n",
        cel_spec_version
    ));

    out
}

fn fetch_cel_spec(
    cache_dir: &Path,
    cel_spec_version: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let archive_url = format!(
        "https://github.com/google/cel-spec/archive/refs/tags/{}.zip",
        cel_spec_version
    );
    let archive_path = cache_dir.join(format!("cel-spec-{}.zip", cel_spec_version));
    let extract_root = cache_dir.join(format!("cel-spec-{}", cel_spec_version));
    let marker = extract_root.join(".extracted_ok");

    if marker.exists() {
        return find_extracted_repo_root(&extract_root);
    }

    fs::create_dir_all(cache_dir)?;

    if !archive_path.exists() {
        let client = Client::builder().build()?;
        let response = client.get(&archive_url).send()?.error_for_status()?;
        let bytes = response.bytes()?;
        fs::write(&archive_path, &bytes)?;
    }

    if extract_root.exists() {
        fs::remove_dir_all(&extract_root)?;
    }
    fs::create_dir_all(&extract_root)?;

    let file = fs::File::open(&archive_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    zip.extract(&extract_root)?;

    fs::write(&marker, b"ok")?;

    find_extracted_repo_root(&extract_root)
}

fn find_extracted_repo_root(extract_root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    for entry in fs::read_dir(extract_root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join("proto").is_dir() {
            return Ok(path);
        }
    }

    Err(format!(
        "Could not locate extracted cel-spec repository root under {}",
        extract_root.display()
    )
    .into())
}

fn sanitize_identifier(input: &str) -> String {
    let mut out = String::new();

    // Convert to snake case.
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out = out.trim_matches('_').to_string();

    if out.is_empty() {
        out = "unnamed".to_string();
    }

    if out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, '_');
    }
    out.into_safe()
}

fn dedupe_identifier(mut identifier: String, counts: &mut HashMap<String, usize>) -> String {
    let key = identifier.clone();
    let count = counts.entry(key.clone()).or_insert(0);
    if *count > 0 {
        identifier = format!("{}_{}", key, count);
    }
    *count += 1;
    identifier
}

fn clear_directory(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    Ok(())
}
