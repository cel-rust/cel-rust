# cel-rust Project Governance

The goal of cel-rust is **a fast, safe, fully spec-compliant Rust implementation of CEL** that stays wire-compatible
with other mainstream CEL runtimes (e.g. `cel-go`).  
The project *does not* extend or fork the CEL language itself; proposals that require changing the upstream spec must
first be raised in the [cel-spec](https://github.com/google/cel-spec) project.

## 1. Principles

1. **Spec fidelity first** – all behaviour must match the latest published CEL language specification.
2. **Security & safety** – memory-safe Rust, secure default feature-flags, and a well-defined responsible-disclosure
   process.
3. **Community-driven** – decisions are made transparently using *lazy consensus* (no sustained objections after 72
   hours) with an escalation path to a vote, mirroring CNCF norms.

## 2. Roles & Responsibilities

| Role            | Typical Expectations                                                             | Powers                                                       |
|-----------------|----------------------------------------------------------------------------------|--------------------------------------------------------------|
| **Contributor** | Opens issues, PRs, docs                                                          | none                                                         |
| **Reviewer**    | Regular contributor with ≥5 non-trivial PRs merged and nominated by a Maintainer | LGTM on PRs, triage issues                                   |
| **Maintainer**  | Deep knowledge of runtime internals; consistent engagement for ≥6 months         | Merge to `main`, cut releases, nominate/remove Reviewers     |
| **TSC Member**  | 3–5 Maintainers elected annually                                                 | Final tie-breaker votes, roadmap ownership, release approval |

## 3. Decision-Making

| Action                                                         | Process                                             |
|----------------------------------------------------------------|-----------------------------------------------------|
| Bug-fixes, docs, small refactors                               | Lazy consensus (72 h)                               |
| API-visible change, new feature flag                           | Requires 2 Maintainer LGTMs and no veto             |
| Breaking change, governance change, adding/removing Maintainer | Formal vote (simple majority of TSC, 1 week window) |
| Security embargo handling                                      | Private Maintainer vote; see `SECURITY.md`          |

A “veto” (`-1`) *must* include a rationale and an alternative proposal. If consensus cannot be reached, the TSC
schedules a vote.

## 4. Meetings & Communication

* GitHub Issues, PRs, and Discussions.

## 5. Roadmap & Releases

* Time-boxed **minor releases** every ~8 weeks.
* **Patch releases** as needed for security or crash bugs.

## 6. Amendments

Governance can be amended by a TSC vote with ≥⅔ majority and one-week public comment period.