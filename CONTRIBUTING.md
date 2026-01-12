# Contributing to cel-rust

Thank you for your interest in contributing to cel-rust! This document outlines our contribution policies and guidelines.

## Before You Start

### Feature Requests and New Features

**Please open an issue first before working on new features.** The project maintainers are moving quickly and making large changes as we work toward full CEL spec compliance. Your PR might:

- Overlap with existing work already in progress
- Move in a direction that conflicts with planned changes
- Be affected by upcoming refactoring

Opening an issue allows us to coordinate and ensure your effort is well-spent. We appreciate your understanding!

## AI Assistance Notice

> [!IMPORTANT]
>
> AI-**assisted** code contributions are allowed but **must be disclosed** in your pull request.

If you use any kind of AI assistance (e.g., GitHub Copilot, ChatGPT, Claude, etc.) while contributing, you must:

1. **Disclose this in your PR description**, including the extent of AI usage (e.g., "Used Copilot for autocomplete" or "Consulted ChatGPT for algorithm design")
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

Failure to disclose AI assistance is disrespectful to maintainers and makes it difficult to properly review your contribution. Please be transparent!

## Pull Request Guidelines

1. **Open an issue first** for features (as noted above)
2. **Keep PRs focused** — one feature or fix per PR
3. **Write tests** for new functionality
4. **Follow existing code style** — run `cargo fmt` and `cargo clippy`
5. **Update documentation** if you're changing public APIs
6. **Write clear commit messages** that explain the "why" not just the "what"

## Code of Conduct

Be respectful, constructive, and collaborative. We're all here to build something useful together.

## Questions?

If you're unsure about anything, just ask! Open an issue or discussion — we're happy to help.

---

Thank you for contributing to cel-rust! 🎉
