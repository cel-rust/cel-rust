# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.1](https://github.com/cel-rust/cel-rust/compare/cel-interpreter-v0.10.0...cel-interpreter-v0.10.1) - 2025-07-24

### Other

- Checked add/sub for timestamps ([#168](https://github.com/cel-rust/cel-rust/pull/168))

## [0.10.0](https://github.com/cel-rust/cel-rust/compare/cel-interpreter-v0.9.1...cel-interpreter-v0.10.0) - 2025-07-23

### Added

- *(antlr)* 🔥 previous parser
- *(antlr)* Good ridance .unwrap()s - part 2 of 2
- *(antlr)* offending whitespaces are fine
- *(antlr)* deal with lexer errors
- *(antlr)* support multiple errors from parsing
- *(antlr)* impl _[_]
- *(antlr)* test only SelectExpr
- *(macros)* Comprehensions
- *(antlr)* Expr are now ID'ed

### Fixed

- Mistakenly Public API changes reverted
- Do not expose internal comprehension var idents
- Do not resolve left operand twice
- has defaults to false on non container types
- don't drop the IdedExpr
- has(_[_]) is that a thing?
- double eval, and lazy eval of right hand expr
- dunno why this changed

### Other

- Updated GH urls to new org ([#158](https://github.com/cel-rust/cel-rust/pull/158))
- Optimizations around member lookups ([#156](https://github.com/cel-rust/cel-rust/pull/156))
- Fixing fuzz test ([#157](https://github.com/cel-rust/cel-rust/pull/157))
- :uninlined_format_args fixes ([#153](https://github.com/cel-rust/cel-rust/pull/153))
- Add basic infrastructure for fuzzing and one target for Value binops ([#152](https://github.com/cel-rust/cel-rust/pull/152))
- Append to lists and strings in place instead of cloning when possible ([#149](https://github.com/cel-rust/cel-rust/pull/149))
- Remove non-standard binary operators ([#147](https://github.com/cel-rust/cel-rust/pull/147))
- Make ExecutionError non-exhaustive ([#148](https://github.com/cel-rust/cel-rust/pull/148))
- Avoid panics due to division by zero and integer overflow ([#145](https://github.com/cel-rust/cel-rust/pull/145))
- Remove redundant clone
- Remove redundant string/error allocations/clones during name resolution
- cargo fmt
- deleted dead code
- add test for 3 args map macro
- deleting fn replaced with macros
- fmt & clippy
- Interpreter adapted to compile using new parser
- simplify function binding magic as an IntoFunction trait ([#133](https://github.com/cel-rust/cel-rust/pull/133))

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
