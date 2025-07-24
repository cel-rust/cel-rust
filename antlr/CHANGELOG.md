# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.10.2](https://github.com/cel-rust/cel-rust/compare/cel-parser-v0.10.1...cel-parser-v0.10.2) - 2025-07-24

### Other

- Using Boxable `ANTLRError` as our Error::source
- Removed non-`Send`+`Sync` errors from `ParseError`

## [0.10.1](https://github.com/cel-rust/cel-rust/compare/cel-parser-v0.10.0...cel-parser-v0.10.1) - 2025-07-23

### Fixed

- Do not expose internal comprehension var idents

### Other

- Updated GH urls to new org ([#158](https://github.com/cel-rust/cel-rust/pull/158))
- :uninlined_format_args fixes ([#153](https://github.com/cel-rust/cel-rust/pull/153))
