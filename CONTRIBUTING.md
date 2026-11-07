# Contributing to cel-rust

Thank you for your interest in contributing to cel-rust! This document outlines our contribution policies and
guidelines.

## Before You Start

### Feature Requests and New Features

**Please open an issue first before working on new features.** The project maintainers are moving quickly and making
large changes as we work toward full CEL spec compliance. Your PR might:

- Overlap with existing work already in progress
- Move in a direction that conflicts with planned changes
- Be affected by upcoming refactoring

Opening an issue allows us to coordinate and ensure your effort is well-spent. We appreciate your understanding!

## Project Priorities

When contributing to cel-rust, please keep in mind our project priorities, in order:

1. **Spec Compliance** — cel-rust implements the [CEL specification](https://github.com/cel-expr/cel-spec) above all
other concerns. Correctness against the specification comes first.

2. **User Experience** — The library should be ergonomic, well-documented, and easy to integrate into Rust projects.

3. **Performance** — We care about performance, but not at the expense of correctness or usability.

These priorities guide our decision-making. For example, if a contribution improves performance but breaks spec
compliance, we'll prioritize the spec. See [GOVERNANCE.md](./GOVERNANCE.md) for roles and how decisions are made.

## Changes to CEL Itself

cel-rust implements CEL; it does not define it. If your proposal would change CEL's syntax, semantics, standard
definitions or an extension library, rather than how cel-rust implements them, raise it in
[cel-expr/cel-spec](https://github.com/cel-expr/cel-spec), following its [governance
process](https://github.com/cel-expr/cel-spec/blob/master/GOVERNANCE.md). cel-rust adopts such changes once they are
part of a cel-spec release.

If cel-rust behaves differently from the specification, that is a bug in cel-rust: open a "Spec conformance gap" issue.
If the gap is already identified by an entry in the `ignored.txt` tests from the conformance test suite, there is no
need to open an issue prior to opening the PR.

## Reporting Security Issues

Please don't open public issues for vulnerabilities. Report them privately through [GitHub private vulnerability
reporting](https://github.com/cel-rust/cel-rust/security/advisories/new).

## AI Assistance Notice

> [!IMPORTANT]
>
> AI-**assisted** code contributions are allowed but **must be disclosed** in your pull request.

If you use any kind of AI assistance (e.g., GitHub Copilot, ChatGPT, Claude, etc.) while contributing, you must:

1. **Disclose this in your PR description**, including the extent of AI usage (e.g., "Used Copilot for autocomplete" or
"Consulted ChatGPT for algorithm design")
2. **Test your changes thoroughly** — you are responsible for understanding and validating the AI-generated code
3. **Be able to explain and defend the changes** — maintainers may ask questions about the implementation

**Example disclosure:**

> This PR was written with assistance from GitHub Copilot for boilerplate code.

Or:

> I consulted Claude to understand the CEL spec requirements, but wrote the implementation myself.

### What We Expect

- **Human accountability** — You must understand the code you're submitting
- **AI assistance ≠ AI generation** — We expect significant human involvement and oversight
- **Code only** — AI-generated documentation, commit messages, and PR descriptions should be reviewed and edited by you

### What We Don't Accept

- PRs where the contributor cannot explain or defend the implementation
- AI-generated responses in issues and discussions (please write in your own words)
- Completely AI-generated code with no human understanding or testing

> [!NOTE]
> Trivial AI features like tab completion don't need to be disclosed. When in doubt, disclose it.

Failure to disclose AI assistance is disrespectful to maintainers and makes it difficult to properly review your
contribution. Please be transparent!

## Pull Request Guidelines

1. **Open an issue first** for features (as noted above)
2. **Keep PRs focused** — one feature or fix per PR
3. **Write tests** for new functionality
4. **Follow existing code style** — run `cargo fmt` and `cargo clippy`
5. **Update documentation** if you're changing public APIs
6. **Write clear commit messages** following the [Conventional Commits](https://www.conventionalcommits.org/) pattern
7. **Sign off your commits** — all commits must include a DCO sign-off (see below)

### Developer Certificate of Origin (DCO)

All commits must be signed off with a Developer Certificate of Origin (DCO). This is verified automatically by GitHub
Actions.

To sign off your commits, add the `-s` flag when committing:

```
bash git commit -s -m "Your commit message"
```

This adds a `Signed-off-by` line to your commit message:

```
Signed-off-by: Your Name <your.email@example.com>
```

By signing off, you certify that you have the right to submit the code under the project's license and agree to the
[Developer Certificate of Origin](https://developercertificate.org/).

## Code of Conduct

Be respectful, constructive, and collaborative. We're all here to build something useful together.

## Questions?

If you're unsure about anything, just ask! Open an issue or discussion — we're happy to help.

---

Thank you for contributing to cel-rust! 🎉
