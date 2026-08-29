use std::path::{Path, PathBuf};

use cargo_toml::Manifest;

const GENERATED_HEADER_PREFIX: &str = "// CEL_SPEC_VERSION: ";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("CARGO_CFG_FEATURE").unwrap_or_default() == "skip-version-check" {
        return Ok(());
    }

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
    let generated_version = manifest_dir.join("src").join("gen").join("version.rs");
    println!("cargo:rerun-if-changed={}", generated_version.display());
    println!("cargo:rerun-if-changed={}", cel_spec_version);
    ensure_generated_version(&generated_version, cel_spec_version)?;
    Ok(())
}

fn ensure_generated_version(
    path: &Path,
    cel_spec_version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!(
                "Missing {}. Run `cargo run -p conformance --features skip-version-check --bin generate`.",
                path.display()
            )
            .into())
        }
        Err(err) => return Err(err.into()),
    };
    let actual = content
        .lines()
        .find_map(|line| line.strip_prefix(GENERATED_HEADER_PREFIX));

    if actual == Some(cel_spec_version) {
        return Ok(());
    }

    let found = actual.unwrap_or("<missing>");
    Err(format!(
        "CEL spec version mismatch in {}: expected {}, found {}. Run `cargo run -p conformance --features skip-version-check --bin generate`.",
        path.display(),
        cel_spec_version,
        found
    )
    .into())
}
