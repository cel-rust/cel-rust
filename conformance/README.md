# CEL Conformance Tests

This crate provides a test harness for running the official CEL conformance tests from the [cel-spec](https://github.com/google/cel-spec) repository against the cel-rust implementation.

## Running the Tests

To run all conformance tests:

```bash
cargo test -p conformance 
```

## Updating conformance tests

The targeted cel-spec version is defined in Cargo.toml `[package.metadata.generate]`. To change the version,
edit the value and run

```bash
cargo run -p conformance --features skip-version-check --bin generate
cargo fmt -p conformance
```

to regenerate test code. Then run the tests to see if there are any new failures. If there are,
either fix code to allow them to pass, or mark them ignored for tracking. To update the list of
ignored tests,

```bash
cargo test -p conformance | cargo run -p conformance --bin update_ignored
```

and regenerate again.
