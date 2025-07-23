# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.1](https://github.com/cel-rust/cel-rust/compare/cel-interpreter-v0.9.0...cel-interpreter-v0.9.1) - 2025-04-29

### Added

- Implement Short-Circuit Evaluation for AND Expressions to Fix Issue #117 ([#118](https://github.com/cel-rust/cel-rust/pull/118))

### Fixed

- improve `Context::add_variable` `Err` type ([#127](https://github.com/cel-rust/cel-rust/pull/127))

### Other

- Add `min` function ([#130](https://github.com/cel-rust/cel-rust/pull/130))
- Fix typos. ([#125](https://github.com/cel-rust/cel-rust/pull/125))
- Add custom Duration and Timestamp types for conversion with serde ([#89](https://github.com/cel-rust/cel-rust/pull/89))
- Export timestamp and duration fn as they were ([#112](https://github.com/cel-rust/cel-rust/pull/112))
- ValueType copy & debug ([#113](https://github.com/cel-rust/cel-rust/pull/113))
- Expose Serialization and ToJson errors ([#114](https://github.com/cel-rust/cel-rust/pull/114))
- Fix compilation without chrono ([#111](https://github.com/cel-rust/cel-rust/pull/111))
- Fix default features, cleanup dependencies & other minor code improvements ([#109](https://github.com/cel-rust/cel-rust/pull/109))
- Added missing timestamp macros ([#106](https://github.com/cel-rust/cel-rust/pull/106))

## [0.9.0](https://github.com/cel-rust/cel-rust/compare/cel-interpreter-v0.8.1...cel-interpreter-v0.9.0) - 2024-10-30

### Other

- Support `.map` over map ([#105](https://github.com/cel-rust/cel-rust/pull/105))
- Detailed parse error ([#102](https://github.com/cel-rust/cel-rust/pull/102))
- Fix `clippy::too_long_first_doc_paragraph` lints. ([#101](https://github.com/cel-rust/cel-rust/pull/101))
- Support empty/default contexts, put chrono/regex behind features ([#97](https://github.com/cel-rust/cel-rust/pull/97))
- Fix `clippy::empty_line_after_doc_comments` lints ([#98](https://github.com/cel-rust/cel-rust/pull/98))
- Allow `.size()` method on types ([#88](https://github.com/cel-rust/cel-rust/pull/88))
- Conformance test fixes ([#79](https://github.com/cel-rust/cel-rust/pull/79))
- Convert CEL values to JSON ([#77](https://github.com/cel-rust/cel-rust/pull/77))
