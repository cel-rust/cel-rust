# CEL Conformance Tests

This crate provides a test harness for running the official CEL conformance tests from the [cel-spec](https://github.com/google/cel-spec) repository against the cel-rust implementation.

## Setup

The conformance tests are pulled in as a git submodule. To initialize the submodule:

```bash
git submodule update --init --recursive
```

## Running the Tests

To run all conformance tests:

```bash
cargo test --package conformance 
```

