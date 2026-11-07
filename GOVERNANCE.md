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
   of a cel-spec release, by moving to that release (see [§3](#3-decision-making)). Applications can still register
   their own functions: the specification leaves that mechanism to implementations.
3. **Safety & security** – memory-safe Rust, secure defaults for feature flags, and private reporting of
   vulnerabilities through
   [GitHub private vulnerability reporting](https://github.com/cel-rust/cel-rust/security/advisories/new).
4. **Community-driven** – decisions are made transparently using *lazy consensus* (no sustained objections after 72
   hours) with an escalation path to a vote, mirroring CNCF norms.

## 2. Roles & Responsibilities

| Role            | Typical Expectations                                                             | Powers                                                       |
|-----------------|----------------------------------------------------------------------------------|--------------------------------------------------------------|
| **Contributor** | Opens issues, PRs, docs                                                          | none                                                         |
| **Reviewer**    | Regular contributor with ≥5 non-trivial PRs merged and nominated by a Maintainer | LGTM on PRs, triage issues                                   |
| **Maintainer**  | Deep knowledge of runtime internals; consistent engagement for ≥6 months         | Merge to `master`, cut releases, nominate/remove Reviewers   |
| **TSC Member**  | 3–5 Maintainers elected annually                                                 | Final tie-breaker votes, roadmap ownership, release approval |

## 3. Decision-Making

| Action                                      | Process                                                                                   |
|---------------------------------------------|-------------------------------------------------------------------------------------------|
| Bug-fixes, docs, small refactors            | Lazy consensus (72 h)                                                                     |
| API-visible change, new feature flag        | Requires 2 Maintainer LGTMs and no veto                                                   |
| Moving to a newer cel-spec release          | Maintainer PR, lazy consensus (72 h); new conformance failures are recorded as known gaps |
| Breaking change, adding/removing Maintainer | Formal vote (simple majority of TSC, 1 week window)                                       |
| Governance change                           | See [§6](#6-amendments)                                                                   |
| Security embargo handling                   | Private Maintainer vote on reports received through private vulnerability reporting       |

A “veto” (`-1`) *must* include a rationale and an alternative proposal. If consensus cannot be reached, the TSC
schedules a vote.

Changes required to conform to the specification are not decided by veto or vote, even when they break cel-rust users.
The processes above only decide how such a change lands: which release carries it, and what migration notes go with it.

## 4. Meetings & Communication

* cel-rust: GitHub Issues, PRs, and Discussions.
* CEL itself: issues in [cel-expr/cel-spec](https://github.com/cel-expr/cel-spec) and the
  `cel-lang-discuss@googlegroups.com` mailing list, as cel-spec's governance process describes.
* Vulnerabilities: [private vulnerability reporting](https://github.com/cel-rust/cel-rust/security/advisories/new),
  never public issues.

## 5. Roadmap & Releases

* Time-boxed **minor releases** every ~8 weeks.
* **Patch releases** as needed for security or crash bugs.

## 6. Amendments

Governance can be amended by a TSC vote with ≥⅔ majority and one-week public comment period.
