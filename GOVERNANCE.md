# cel-rust Project Governance

cel-rust is a Rust implementation of the Common Expression Language (CEL). This document governs the implementation.
CEL itself (its syntax, semantics, standard definitions, extension libraries and conformance tests) is governed by
[cel-expr/cel-spec](https://github.com/cel-expr/cel-spec) under its own
[governance process](https://github.com/cel-expr/cel-spec/blob/master/GOVERNANCE.md), and cel-rust defers to it on all
of these.

The goal of cel-rust is **a fully spec-conformant Rust implementation of CEL**. Within that constraint, it aims to be
safe, ergonomic and fast.

## 1. Principles

1. **The specification comes first** – cel-rust implements the CEL specification above all other concerns, including
   performance, API ergonomics and the stability of cel-rust's own behaviour. "The specification" is the cel-spec
   release that cel-rust's conformance suite is pinned to (see [conformance](./conformance/README.md)). Where another
   CEL implementation disagrees with the specification, cel-rust follows the specification.
2. **CEL is changed upstream, not here** – cel-rust does not add syntax, change semantics, or extend the standard
   definitions or the extension libraries. Changes to CEL start as a feature request in cel-spec and are reviewed by
   the CEL Language Council, as cel-spec's governance process describes. cel-rust adopts such a change once it is part
   of a cel-spec release, by moving to that release (see [§4](#4-decision-making)). Applications can still register
   their own functions: the specification leaves that mechanism to implementations.
3. **Safety & security** – memory-safe Rust, secure defaults for feature flags, and private reporting of
   vulnerabilities through
   [GitHub private vulnerability reporting](https://github.com/cel-rust/cel-rust/security/advisories/new).
4. **Community-driven** – decisions are made in the open, on GitHub issues, pull requests and Discussions.
   Disagreements that can't be settled there are decided by the Maintainers (see [§4](#4-decision-making)).

## 2. Roles & Responsibilities

Anyone can contribute by opening issues, Discussions and pull requests. Triaging issues, and approving and merging pull
requests, is done by two roles:

| Role           | How the role is given                                             | Responsibilities                                                                                    |
|----------------|-------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------|
| **Committer**  | Invited by the Maintainers, based on participation in the project | Write access to the repository: triage issues, review, approve and merge pull requests              |
| **Maintainer** | Added by the Maintainers (see [§4](#4-decision-making))           | Everything a Committer does, plus inviting Committers and the decisions in [§4](#4-decision-making) |

## 3. Issues & Triage

1. New issues are labelled `triage/needed`; the issue templates apply this label.
2. A Committer or Maintainer triages the issue: they make sure it is understood, then accept it or close it. While the
   expected outcome is still unclear, the issue is labelled `needs/agreement`.
3. An accepted issue is labelled `triage/accepted` and given a target milestone for when it should be fixed.
4. Don't start work on an issue until it has been accepted.

## 4. Decision-Making

| Action                             | Process                                                                             |
|------------------------------------|-------------------------------------------------------------------------------------|
| Any pull request                   | One approving review from a Committer or Maintainer                                 |
| Moving to a newer cel-spec release | A pull request like any other; new conformance failures are recorded as known gaps  |
| Inviting a Committer               | The Maintainers, based on the person's participation in the project                 |
| Adding or removing a Maintainer    | Simple majority of Maintainers                                                      |
| Governance change                  | See [§7](#7-amendments)                                                             |
| Security embargo handling          | Private Maintainer vote on reports received through private vulnerability reporting |

CI labels pull requests that break the public Rust API `api/bump-version`, so reviewers can see when a change needs a
major version bump.

A review that requests changes *must* include a rationale and an alternative proposal. If the author and reviewers
can't agree, the Maintainers decide by simple majority.

Changes required to conform to the specification are not blocked by review or decided by vote, even when they break
cel-rust users. The processes above only decide how such a change lands: which release carries it, and what migration
notes go with it.

## 5. Meetings & Communication

* cel-rust: GitHub Issues, PRs, and Discussions.
* CEL itself: issues in [cel-expr/cel-spec](https://github.com/cel-expr/cel-spec) and the
  `cel-lang-discuss@googlegroups.com` mailing list, as cel-spec's governance process describes.
* Vulnerabilities: [private vulnerability reporting](https://github.com/cel-rust/cel-rust/security/advisories/new),
  never public issues.

## 6. Roadmap & Releases

* Time-boxed **minor releases** every ~8 weeks.
* **Patch releases** as needed for security or crash bugs.

## 7. Amendments

Governance can be amended by a ≥⅔ majority of Maintainers, after a one-week public comment period.
