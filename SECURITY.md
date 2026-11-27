# Security Policy

## Reporting a vulnerability

Please don't open a public issue, pull request or discussion for a vulnerability. Report it privately through
[GitHub private vulnerability reporting](https://github.com/cel-rust/cel-rust/security/advisories/new).

Include what you can of:

- the version (or commit) of the `cel` crate, and the Cargo features enabled;
- the CEL expression, and the variables or values bound when it is evaluated;
- what happens (panic message, stack overflow, time or memory used), and what you expected.

Reports are handled as set out in [GOVERNANCE.md](./GOVERNANCE.md#4-decision-making), where the maintainers decide
the embargo by a private vote. Fixed vulnerabilities are published as
[GitHub security advisories](https://github.com/cel-rust/cel-rust/security/advisories).

## Supported versions

Only the latest minor release of the `cel` crate are guaranteed to receive security fixes, as a patch release of that
minor. Earlier minor releases _might not_ get backports: try to upgrade to the latest minor release to receive a fix.

## Scope

cel-rust is often used to evaluate expressions, or data, that its host application doesn't fully control. These are
vulnerabilities:

- a panic, abort or stack overflow reachable from a CEL expression, from the values bound to it, or from displaying
  the errors it returns;
- a way around a limit the host has set, such as the parser's `max_recursion_depth`, the iteration budget of
  `RuntimeOptions::with_max_iterations`, or an interrupt set with `Context::set_interrupt`;
- memory unsafety of any kind.

These are not, on their own, and belong in a regular issue:

- an expression that runs long or uses a lot of memory when no limit is set. CEL puts no bound on what an expression
  costs; the parser limits, the iteration budget and interrupts (such as a `Deadline`) are there to set one;
- a result that differs from the CEL specification: open a "Spec conformance gap" issue;
- a bug in a function or type that the host application registers itself.
