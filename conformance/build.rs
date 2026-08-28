fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Tell cargo to rerun this build script if the proto files change
    println!("cargo:rerun-if-changed=../cel-spec/proto");

    // Configure prost to generate Rust code from proto files
    let mut config = prost_build::Config::new();
    config.protoc_arg("--experimental_allow_proto3_optional");

    // Add well-known types from prost-types
    config.bytes(["."]);

    // Generate FileDescriptorSet for prost-reflect runtime type resolution
    let descriptor_path =
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("file_descriptor_set.bin");
    config.file_descriptor_set_path(&descriptor_path);

    // Compile the proto files
    config.compile_protos(
        &[
            "../cel-spec/proto/cel/expr/value.proto",
            "../cel-spec/proto/cel/expr/syntax.proto",
            "../cel-spec/proto/cel/expr/checked.proto",
            "../cel-spec/proto/cel/expr/eval.proto",
            "../cel-spec/proto/cel/expr/conformance/test/simple.proto",
            "../cel-spec/proto/cel/expr/conformance/proto2/test_all_types.proto",
            "../cel-spec/proto/cel/expr/conformance/proto2/test_all_types_extensions.proto",
            "../cel-spec/proto/cel/expr/conformance/proto3/test_all_types.proto",
        ],
        &["../cel-spec/proto"],
    )?;

    Ok(())
}
