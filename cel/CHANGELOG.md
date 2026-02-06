# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.13.0](https://github.com/cel-rust/cel-rust/compare/v0.12.0...v0.13.0) - 2026-02-06

### Added

- *(dyn Val)* Fix `OpaqueVal` and `TryFrom` for actual opaques
- *(dyn Val)* Making `Val: Send + Sync`... for now?
- *(dyn Val)* `CelDuration` & `CelTimestamp` use chrono again
- *(dyn Val)* Avoid cloning key on Map access
- *(dyn Val)* Inject `Box<dyn Val>` straight into context, bypassing `From::<Value>`
- *(dyn Val)* All literals as dyn Val
- *(dyn Val)* [**breaking**] Context is NOT `Send` anymore, but stores `dyn Val`s
- *(dyn Val)* Bench now uses only that `resolve_val`
- *(dyn Val)* Optional support in `Expr::Map`
- *(dyn Val)* `Expr::List` flattens `CelOptional::None`
- *(dyn Val)* Deal with OPT_INDEX
- *(dyn Val)* Full `Optional` type
- *(dyn Val)* Support for generic Opaque
- *(dyn Val)* need neg durations
- *(dyn Val)* Comparer for Numbers
- *(dyn Val)* need max/min and no neg duration
- *(dyn Val)* Map impl into Indexer
- *(dyn Val)* Binary ops return UnsupportedBinaryOperator on Err
- *(dyn Val)* UInt impl (Adder|Comparer|Divider|Modder|Multiplier|Subtractor)
- *(dyn Val)* Timestamp impl (Adder|Comparer|Subtractor)
- *(dyn Val)* String impl (Adder|Comparer)
- *(dyn Val)* Int impl Negator
- *(dyn Val)* Duration impl (Adder|Comparer|Substrator)
- *(dyn Val)* Double impl (Adder|Comparer|Divider|Multiplier|Negator|Subtractor)
- *(dyn Val)* Adder for Bytes
- *(dyn Val)* Bytes impl (Adder|Comparer)
- *(dyn Val)* Bool impl (Comparer|Negator)
- *(dyn Val)* DefaultMap index lookup errors
- *(dyn Val)* impl Adder for DefaultList
- *(dyn Val)* Overflow safe math ops on int
- *(dyn Val)* fixed all maths ops operands
- *(dyn Val)* impl Indexer for DefaultMap
- *(dyn Val)* Trailing ::resolve's replaced
- *(dyn Val)* Implement Val.equals for all types
- *(dyn Val)* new iterator et al trait sigs and impl for List & Map
- *(dyn Val)* DefaultMap as_container
- *(dyn Val)* Interpreter _should_ be ready, other than optional
- *(dyn Val)* Expr::Map, missing optional still
- *(dyn Val)* DefaultList Indexer no more panics
- *(dyn Val)* Indexer impl for DefaultList to support UInt
- *(dyn Val)* wrapping in Cow... where needed for now
- *(dyn Val)* reflective impl for Value to dyn Val
- *(dyn Val)* `TryFrom<&dyn Val>` container types added
- *(dyn Val)* Basic Map support
- *(dyn Val)* CelList construction rework, no `::new`
- *(dyn Val)* Expr::Select
- *(dyn Val)* Expr::Ident in interpreter using try_into
- *(dyn Val)* fn calls using try_into
- *(dyn Val)* `!_`, `-_` & `@not_strictly_false` in interpreter
- *(dyn Val)* `@in` operator support
- *(dyn Val)* align all type impl of traits
- *(dyn Val)* Bytes, Duration & Timestamp from/to dyn Val/Value
- *(dyn Val)* aligned TryFrom from/to dyn Val/Value
- *(dyn Val)* `TryFrom<Value> for Box<dyn Val>`
- *(dyn Val)* Basic `Modder` support in interpreter & types::INT
- *(dyn Val)* Basic div support in interpreter & types::INT
- *(dyn Val)* No default impl for CEL traits
- *(dyn Val)* `Divider` trait basic support
- *(dyn Val)* `Subtractor` trait basic support
- *(dyn Val)* Basic cmp support in interpreter & types::INT
- *(dyn Val)* Adder.add failable
- *(dyn Val)* DefaultList tests and removed support for UINT indexes
- *(dyn Val)* Indexer has proper Result/Err return sigs
- *(dyn Val)* list expr
- *(dyn Val)* fix to amend testing the Cow approach
- *(dyn Val)* default Indexer::steal impl
- *(dyn Val)* faster steal from Vec in Indexer
- *(dyn Val)* Very lazy List indexing with Cow
- *(dyn Val)* testing the Cow approach... unsure
- *(dyn Val)* todo impl optional again
- *(dyn Val)* Index interpreter
- *(dyn Val)* fixing bool logic
- *(dyn Val)* Listerals to Cow<dyn Val>
- *(dyn Val)* all types and base are there, now fix objects.rs 🫣
- *(dyn Val)* fix to `Adder` API
- *(dyn Val)* `impl Val for Int` and fix to `Indexer` API
- *(dyn Val)* `impl Val for Int`
- *(dyn Val)* `impl Val for Err`
- *(dyn Val)* list
- *(dyn Val)* cond, or, and, eq & neq do use CelVal
- *(dyn Val)* Box<Self>
- *(dyn Val)* LiteralValue
- *(dyn Val)* Basic dyn Val and matching types

### Fixed

- [**breaking**] no type coersion for map index access
- [**breaking**] NoSuchOverload when indexing into String
- *(bench)* Make sure expression compiles

### Other

- *(deps)* replaced `paste` with newer `pastey`
- *(tests)* smaller stack on max depth test
- explain `fn cast_boxed<T: Val>`, and safety related checks
- rustfmt
- Only lazily create `ok_or`'s `Err`
- slightly faster `Adder for String`
- let's use a single lifetime on ::resolve_val
- Use iterator trait on comprehensions
- deleted `CelVal`
- clippy
- Adder trait sig
- Int as Comparer
- no need for `as` cast in as_* impl
- *(dyn Val)* clippy clean ups
- *(dyn Val)* extracted `cast_boxed` fn
- *(dyn Val)* added `TryFrom<&dyn Val> for Value`
- clean ups
- `PartialEq` for `Val` delegating to `Val::equals`
- export all types::{type} as Cel{type}
- *(dyn Val)* no into_any, CelVal is not a Val
- fixing up type sigs and original resolve
- *(fmt)* rustfmt
- Isolated interpreter special cases of ops
- :Int copy & default
- use key references
- update minimal rust version to 1.86

## [0.12.0](https://github.com/cel-rust/cel-rust/compare/v0.11.6...v0.12.0) - 2025-12-29

### Added

- *(Optional)* Initial support
- *(opaque)* docs
- *(opaque)* PR comments addressed
- *(Opaque)* json support
- *(Opaque)* No indirection, straight holds trait OpaqueValue
- *(opaque)* no need for as_any
- *(opaque)* Equality of opaques
- *(opaque)* wire function example
- *(opaque)* Adds support for `OpaqueValue`s
- *(parser)* Proper support for comments

### Fixed

- fix formatting
- fix logic and function naming
- fixup test
- *(opaque)* Refactor OpaqueValue to simply Opaque
- account for feature chrono in Debug
- remove dep
- *(arbitrary)* no more arbitratry in the main crate
- *(arbitrary)* less pervasive usage

### Other

- Merge pull request #240 from Rick-Phoenix/bytes-support
- Update README example to use CEL 0.12.0 ([#242](https://github.com/cel-rust/cel-rust/pull/242))
- Support get{Hours,Minutes,Seconds,Milliseconds} on duration
- Merge pull request #234 from adam-cattermole/optional
- Optional tests use Parser directly
- Initialize lists and maps with optionals in interpreter
- Handle optional index in interpreter
- Fix should error on missing map key
- Handle optional select in interpreter
- Handle optionals in lists in parser
- Handle optional struct/map initializer in parser
- Add optional visit_Select/Index to parser
- Add enable_optional_syntax option to parser
- Add orValue function for optional
- Add or function for optional
- Add hasValue function for optional
- Add value function for optional
- Add optional.ofNonZeroValue
- Add optional.of
- Documentation and pass reference
- Add new way to resolve variables
- default to BTree's instead of HashMap ([#231](https://github.com/cel-rust/cel-rust/pull/231))
- avoid cloning function args and name ([#228](https://github.com/cel-rust/cel-rust/pull/228))
- avoid double resolving single-arg func calls ([#227](https://github.com/cel-rust/cel-rust/pull/227))
- move
- minor tweaks to make usage more ergonomic
- Fix Context docstring to reference new_inner_scope instead of clone ([#221](https://github.com/cel-rust/cel-rust/pull/221))

## [0.11.6](https://github.com/cel-rust/cel-rust/compare/v0.11.5...v0.11.6) - 2025-10-23

### Added

- *(recursion)* Threshold operates on language constructs

### Fixed

- avoid panic'ing on somehow bad parser input ([#215](https://github.com/cel-rust/cel-rust/pull/215))
- regenerated parser
- better contract to max_recursion_depth
- new antlr4rust

### Other

- Bump README CEL version to 0.11.6
- updated to latest antlr4rust and generated code
- added notes on generating the parser
- updated antlr4rust dependency
- wip

## [0.11.5](https://github.com/cel-rust/cel-rust/compare/cel-v0.11.4...cel-v0.11.5) - 2025-10-15

### Fixed

- support 1.82 onwards ([#207](https://github.com/cel-rust/cel-rust/pull/207))

### Other

- Update README.md

## [0.11.4](https://github.com/cel-rust/cel-rust/compare/cel-v0.11.3...cel-v0.11.4) - 2025-10-09

### Fixed

- antlr4rust update, and fix to allow for linefeed ParseErr
- *(parser)* Gets rid of ever invoking Visitable with no impl
- *(string)* String index accesses err out
- *(clippy)* manual_is_multiple_of
- *(parser)* Stop traversing AST on PrimaryContextAll::Error

### Other

- add coverage
- Merge pull request #199 from cel-rust/issue-198

## [0.11.3](https://github.com/cel-rust/cel-rust/compare/cel-v0.11.2...cel-v0.11.3) - 2025-10-02

### Fixed

- *(parsing)* stop navigating AST on err

## [0.11.2](https://github.com/cel-rust/cel-rust/compare/cel-v0.11.1...cel-v0.11.2) - 2025-09-19

### Other

- updated antlr4rust to v0.3.0-rc1 explicitly ([#189](https://github.com/cel-rust/cel-rust/pull/189))

## [0.11.1](https://github.com/cel-rust/cel-rust/compare/cel-v0.11.0...cel-v0.11.1) - 2025-08-20

### Fixed

- *(clippy)* hiding a lifetime that's elided elsewhere is confusing
- Added proper `ExecutionError::NoSuchOverload`
- no bool coercion

### Other

- Merge pull request #185 from alexsnaps/cleanup-coerce-into-bool

## [0.11.0](https://github.com/cel-rust/cel-rust/compare/cel-v0.10.0...cel-v0.11.0) - 2025-08-06

### Other

- Fix CEL readme ([#180](https://github.com/cel-rust/cel-rust/pull/180))
- Merge pull request #154 from alexsnaps/types
- Fix usage of identifier in custom functions ([#174](https://github.com/cel-rust/cel-rust/pull/174))
- Merge pull request #169 from cgettys-microsoft/shrink-expr-01
- Make Program expose the Expr ([#171](https://github.com/cel-rust/cel-rust/pull/171))
- unused struct, using ([#170](https://github.com/cel-rust/cel-rust/pull/170))

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
