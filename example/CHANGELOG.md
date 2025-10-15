# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/cel-rust/cel-rust/compare/example-v0.1.0...example-v0.1.1) - 2025-10-15

### Added

- *(antlr)* support multiple errors from parsing

### Other

- Consolidate parser and interpreter into single `cel` crate ([#167](https://github.com/cel-rust/cel-rust/pull/167))
- Clippy
- Added anyhow example
- Fix default features, cleanup dependencies & other minor code improvements ([#109](https://github.com/cel-rust/cel-rust/pull/109))
- Support empty/default contexts, put chrono/regex behind features ([#97](https://github.com/cel-rust/cel-rust/pull/97))
- Convert CEL values to JSON ([#77](https://github.com/cel-rust/cel-rust/pull/77))
- Add an Axum example ([#57](https://github.com/cel-rust/cel-rust/pull/57))
- Feat/serializer ([#33](https://github.com/cel-rust/cel-rust/pull/33))
- Supporting thread-safe execution ([#22](https://github.com/cel-rust/cel-rust/pull/22))
- Updated integer types from 32-bit to 64-bit
- Cleanup, linting, and more examples
- Fixing a few quirks and adding more function examples
- General clean up
- Ported existing functions to use magic params
- First pass of function value resolvers
- Bunch of renaming and cleanup
- Added function context for better function utilities ([#7](https://github.com/cel-rust/cel-rust/pull/7))
- Added some examples about how to use the library ([#5](https://github.com/cel-rust/cel-rust/pull/5))
