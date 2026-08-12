use crate::ExecutionError;
use std::time::{Duration, Instant};

/// Per-invocation monotonic execution budget for CEL evaluation.
///
/// A budget is cooperative: the interpreter checks the deadline before resolving
/// AST nodes, around function dispatch, and on each comprehension iteration.
/// Host callbacks and other long-running operations that do not return to the
/// interpreter cannot be preempted mid-call.
///
/// Budgets are intended to be attached to a single `Program::execute` /
/// [`Program::execute_with_budget`](crate::Program::execute_with_budget) call.
/// They must not be stored as shared mutable state on a compiled
/// [`Program`](crate::Program).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionBudget {
    deadline: Option<Instant>,
}

impl ExecutionBudget {
    /// No deadline; evaluation runs until it completes or fails for another reason.
    pub const fn unlimited() -> Self {
        Self { deadline: None }
    }

    /// Expire at an absolute monotonic instant.
    pub fn with_deadline(deadline: Instant) -> Self {
        Self {
            deadline: Some(deadline),
        }
    }

    /// Expire after `timeout` from now, using a monotonic clock.
    ///
    /// A zero duration creates an already-expired (or immediately-expiring)
    /// budget, which is useful for deterministic tests.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            deadline: Some(Instant::now() + timeout),
        }
    }

    /// Returns `true` when no deadline is configured.
    pub fn is_unlimited(&self) -> bool {
        self.deadline.is_none()
    }

    /// Returns the configured monotonic deadline, if any.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Returns [`ExecutionError::DeadlineExceeded`] when the deadline has been
    /// reached or passed.
    #[inline]
    pub fn check(&self) -> Result<(), ExecutionError> {
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                return Err(ExecutionError::DeadlineExceeded);
            }
        }
        Ok(())
    }
}
