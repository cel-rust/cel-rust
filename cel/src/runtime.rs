//! Per-evaluation runtime state: cooperative interruption and iteration budgets.
//!
//! Evaluation of a CEL expression is bounded by two independent mechanisms:
//!
//! - an [`Interrupt`] handle, set per evaluation on the [`Context`](crate::Context)
//!   with [`Context::set_interrupt`](crate::Context::set_interrupt), that the
//!   interpreter polls while iterating comprehensions (`all`, `exists`, `map`,
//!   `filter`, ...);
//! - an iteration budget, configured on the [`Env`](crate::Env) through
//!   [`RuntimeOptions`], that caps the total number of comprehension iterations
//!   performed by a single evaluation, nested comprehensions included.
//!
//! When either trips, evaluation aborts with
//! [`ExecutionError::Interrupted`](crate::ExecutionError::Interrupted) or
//! [`ExecutionError::IterationBudgetExceeded`](crate::ExecutionError::IterationBudgetExceeded).
//! Unlike ordinary CEL errors, these are never absorbed by `||`, `&&`, or
//! optional accessors: once tripped, the evaluation as a whole fails.

use crate::ExecutionError;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant};

/// A cancellation signal consulted by the interpreter during evaluation.
///
/// Implement this for whatever drives cancellation in your application: a
/// flag flipped from another thread, a deadline, a runtime's cancellation
/// token. The interpreter polls it while iterating comprehensions, and custom
/// functions can poll it through
/// [`FunctionContext::is_interrupted`](crate::FunctionContext::is_interrupted).
///
/// Implementations are provided for [`AtomicBool`], [`Deadline`], and any
/// `Fn() -> bool + Send + Sync` closure.
///
/// # Example
/// ```
/// use cel::{Context, Deadline, ExecutionError, Program};
/// use std::time::Duration;
///
/// let program = Program::compile("[1, 2, 3].all(x, x > 0)").unwrap();
/// let deadline = Deadline::after(Duration::from_millis(50));
/// let mut ctx = Context::default();
/// ctx.set_interrupt(&deadline);
/// assert_eq!(program.execute(&ctx), Ok(true.into()));
/// ```
pub trait Interrupt: Send + Sync {
    /// Returns `true` once evaluation should stop.
    fn is_interrupted(&self) -> bool;
}

impl Interrupt for AtomicBool {
    fn is_interrupted(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}

impl<F> Interrupt for F
where
    F: Fn() -> bool + Send + Sync,
{
    fn is_interrupted(&self) -> bool {
        self()
    }
}

/// An [`Interrupt`] that trips once a point in time has passed.
///
/// # Example
/// ```
/// use cel::Deadline;
/// use std::time::Duration;
///
/// let deadline = Deadline::after(Duration::from_secs(1));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline(Instant);

impl Deadline {
    /// A deadline that trips at the given instant.
    pub fn at(instant: Instant) -> Self {
        Deadline(instant)
    }

    /// A deadline that trips once `timeout` has elapsed from now.
    ///
    /// Saturates at the far future if `Instant::now() + timeout` would overflow.
    pub fn after(timeout: Duration) -> Self {
        let now = Instant::now();
        Deadline(
            now.checked_add(timeout)
                .unwrap_or(now + Duration::from_secs(u32::MAX as u64)),
        )
    }

    /// The instant at which this deadline trips.
    pub fn instant(&self) -> Instant {
        self.0
    }
}

impl Interrupt for Deadline {
    fn is_interrupted(&self) -> bool {
        Instant::now() >= self.0
    }
}

/// Runtime policy shared by every evaluation performed under an [`Env`](crate::Env).
///
/// # Example
/// ```
/// use cel::{Context, Env, ExecutionError, Program, RuntimeOptions};
/// use std::sync::Arc;
///
/// let mut env = Env::stdlib();
/// env.set_options(RuntimeOptions::default().with_max_iterations(5));
/// let ctx = Context::with_env(Arc::new(env));
///
/// let program = Program::compile("[1, 2, 3].all(x, x > 0)").unwrap();
/// assert_eq!(program.execute(&ctx), Ok(true.into()));
///
/// let program = Program::compile("[1, 2, 3, 4, 5, 6].all(x, x > 0)").unwrap();
/// assert_eq!(
///     program.execute(&ctx),
///     Err(ExecutionError::IterationBudgetExceeded { limit: 5 })
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RuntimeOptions {
    max_iterations: u64,
    interrupt_check_frequency: u64,
}

impl Default for RuntimeOptions {
    /// No iteration budget, and the interrupt handle polled on every iteration.
    fn default() -> Self {
        RuntimeOptions {
            max_iterations: 0,
            interrupt_check_frequency: 1,
        }
    }
}

impl RuntimeOptions {
    /// Caps the total number of comprehension iterations a single evaluation may
    /// perform, across all comprehensions in the expression, nested ones included.
    ///
    /// Zero, the default, means unlimited.
    pub fn with_max_iterations(mut self, max_iterations: u64) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Polls the [`Interrupt`] handle every `frequency` comprehension iterations.
    ///
    /// The default is one, polling on every iteration. Zero is clamped to one:
    /// setting a frequency never disables interruption.
    pub fn with_interrupt_check_frequency(mut self, frequency: u64) -> Self {
        self.interrupt_check_frequency = frequency.max(1);
        self
    }

    /// The iteration budget, zero meaning unlimited.
    pub fn max_iterations(&self) -> u64 {
        self.max_iterations
    }

    /// How many comprehension iterations run between polls of the interrupt handle.
    pub fn interrupt_check_frequency(&self) -> u64 {
        self.interrupt_check_frequency
    }
}

const ABORT_NONE: u8 = 0;
const ABORT_BUDGET: u8 = 1;
const ABORT_INTERRUPTED: u8 = 2;

/// Per-evaluation state, created by the outermost evaluation entry point and
/// shared by every nested scope and re-entrant call underneath it.
///
/// This type is opaque: it is managed entirely by the interpreter and exposes
/// no public API. It is only nameable so that it may appear as a field of
/// [`Context::Child`](crate::Context::Child).
///
/// Counters are atomics rather than `Cell`s only so that `Context` stays `Sync`;
/// evaluation itself is single-threaded, so relaxed ordering is sufficient.
pub struct Frame<'a> {
    interrupt: Option<&'a dyn Interrupt>,
    max_iterations: u64,
    check_frequency: u64,
    iterations: AtomicU64,
    polls: AtomicU64,
    abort: AtomicU8,
}

impl<'a> Frame<'a> {
    pub(crate) fn new(options: &RuntimeOptions, interrupt: Option<&'a dyn Interrupt>) -> Self {
        Frame {
            interrupt,
            max_iterations: options.max_iterations,
            check_frequency: options.interrupt_check_frequency.max(1),
            iterations: AtomicU64::new(0),
            polls: AtomicU64::new(0),
            abort: AtomicU8::new(ABORT_NONE),
        }
    }

    /// Accounts for one comprehension iteration: enforces the budget and polls
    /// the interrupt handle at the configured frequency.
    ///
    /// Once an abort has been recorded, every subsequent call fails immediately.
    #[inline]
    pub(crate) fn tick(&self) -> Result<(), ExecutionError> {
        if let Some(err) = self.abort_error() {
            return Err(err);
        }
        let iterations = self.iterations.fetch_add(1, Ordering::Relaxed) + 1;
        if self.max_iterations > 0 && iterations > self.max_iterations {
            self.record(ABORT_BUDGET);
            return Err(self.abort_error().expect("abort was just recorded"));
        }
        if let Some(interrupt) = self.interrupt {
            let polls = self.polls.fetch_add(1, Ordering::Relaxed) + 1;
            if polls % self.check_frequency == 0 && interrupt.is_interrupted() {
                self.record(ABORT_INTERRUPTED);
                return Err(ExecutionError::Interrupted);
            }
        }
        Ok(())
    }

    /// Polls the interrupt handle directly, bypassing the frequency gate.
    ///
    /// Observing an interrupt is sticky: the evaluation will fail with
    /// [`ExecutionError::Interrupted`] even if the caller goes on to return a value.
    pub(crate) fn observe_interrupt(&self) -> bool {
        if self.abort.load(Ordering::Relaxed) == ABORT_INTERRUPTED {
            return true;
        }
        match self.interrupt {
            Some(interrupt) if interrupt.is_interrupted() => {
                self.record(ABORT_INTERRUPTED);
                true
            }
            _ => false,
        }
    }

    /// Replaces a result with the recorded abort, if any.
    ///
    /// This is the backstop guaranteeing an abort surfaces even if the error
    /// value was swallowed somewhere along the way.
    pub(crate) fn finish<T>(&self, result: Result<T, ExecutionError>) -> Result<T, ExecutionError> {
        match self.abort_error() {
            Some(err) => Err(err),
            None => result,
        }
    }

    /// Records an abort. An interrupt always wins over a budget overrun, since
    /// the embedder asked for it explicitly.
    fn record(&self, abort: u8) {
        if abort == ABORT_INTERRUPTED {
            self.abort.store(ABORT_INTERRUPTED, Ordering::Relaxed);
        } else {
            let _ = self.abort.compare_exchange(
                ABORT_NONE,
                abort,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    fn abort_error(&self) -> Option<ExecutionError> {
        match self.abort.load(Ordering::Relaxed) {
            ABORT_INTERRUPTED => Some(ExecutionError::Interrupted),
            ABORT_BUDGET => Some(ExecutionError::IterationBudgetExceeded {
                limit: self.max_iterations,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Context, Env, ExecutionError, FunctionContext, Program, ResolveResult, Value};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn budgeted(max_iterations: u64) -> Context<'static> {
        let mut env = Env::stdlib();
        env.set_options(RuntimeOptions::default().with_max_iterations(max_iterations));
        Context::with_env(Arc::new(env))
    }

    fn run(ctx: &Context, src: &str) -> ResolveResult {
        Program::compile(src).expect("must parse").execute(ctx)
    }

    fn run_optional(ctx: &Context, src: &str) -> ResolveResult {
        let expr = crate::parser::Parser::new()
            .enable_optional_syntax(true)
            .parse(src)
            .expect("must parse");
        ctx.resolve(&expr)
    }

    fn budget_exceeded(limit: u64) -> ResolveResult {
        Err(ExecutionError::IterationBudgetExceeded { limit })
    }

    #[test]
    fn budget_off_by_default() {
        let ctx = Context::default();
        assert_eq!(ctx.env().options().max_iterations(), 0);
        assert_eq!(ctx.env().options().interrupt_check_frequency(), 1);
        let list: Vec<i64> = (0..20_000).collect();
        let mut ctx = Context::default();
        ctx.add_variable("list", list).unwrap();
        assert_eq!(run(&ctx, "list.all(x, x >= 0)"), Ok(true.into()));
    }

    #[test]
    fn zero_frequency_clamps_to_one() {
        let options = RuntimeOptions::default().with_interrupt_check_frequency(0);
        assert_eq!(options.interrupt_check_frequency(), 1);
    }

    #[test]
    fn budget_trips_at_limit() {
        let ctx = budgeted(3);
        assert_eq!(run(&ctx, "[1, 2, 3].all(x, x > 0)"), Ok(true.into()));
        assert_eq!(run(&ctx, "[1, 2, 3, 4].all(x, x > 0)"), budget_exceeded(3));
    }

    #[test]
    fn budget_is_global_across_nested_comprehensions() {
        // 2 outer + 2 * 2 inner = 6 iterations
        let ctx = budgeted(6);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, [1, 2].all(y, y > 0))"),
            Ok(true.into())
        );
        let ctx = budgeted(5);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, [1, 2].all(y, y > 0))"),
            budget_exceeded(5)
        );
    }

    #[test]
    fn budget_is_global_across_sibling_comprehensions() {
        let ctx = budgeted(4);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0) && [1, 2].all(x, x > 0)"),
            Ok(true.into())
        );
        let ctx = budgeted(3);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0) && [1, 2].all(x, x > 0)"),
            budget_exceeded(3)
        );
    }

    #[test]
    fn budget_is_per_evaluation() {
        let ctx = budgeted(3);
        for _ in 0..5 {
            assert_eq!(run(&ctx, "[1, 2, 3].all(x, x > 0)"), Ok(true.into()));
        }
    }

    #[test]
    fn budget_is_not_absorbed_by_logical_or() {
        let ctx = budgeted(1);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0) || true"),
            budget_exceeded(1)
        );
        assert_eq!(
            run(&ctx, "false || [1, 2].all(x, x > 0)"),
            budget_exceeded(1)
        );
    }

    #[test]
    fn budget_is_not_absorbed_by_logical_and() {
        let ctx = budgeted(1);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0) && false"),
            budget_exceeded(1)
        );
        assert_eq!(
            run(&ctx, "true && [1, 2].all(x, x > 0)"),
            budget_exceeded(1)
        );
    }

    #[test]
    fn budget_is_not_absorbed_by_all_macro_predicate() {
        // `all` wraps its predicate in @not_strictly_false, which turns errors into `true`.
        let ctx = budgeted(2);
        assert_eq!(
            run(&ctx, "[1].all(x, [1, 2].all(y, y > 0))"),
            budget_exceeded(2)
        );
    }

    #[test]
    fn budget_is_not_absorbed_by_optional_accessors() {
        let ctx = budgeted(1);
        assert_eq!(
            run_optional(&ctx, "{'a': 1}[?'a'].orValue([1, 2].all(x, x > 0))"),
            budget_exceeded(1)
        );
        assert_eq!(
            run_optional(&ctx, "[[1, 2].all(x, x > 0)][?0].hasValue()"),
            budget_exceeded(1)
        );
        assert_eq!(
            run_optional(&ctx, "{'a': [1, 2].all(x, x > 0)}.?a.hasValue()"),
            budget_exceeded(1)
        );
    }

    #[test]
    fn budget_is_not_absorbed_by_ternary() {
        let ctx = budgeted(1);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0) ? 1 : 2"),
            budget_exceeded(1)
        );
    }

    #[test]
    fn budget_is_shared_with_reentrant_function_calls() {
        fn reenter(ftx: &FunctionContext) -> ResolveResult {
            let program = Program::compile("[1, 2, 3].all(x, x > 0)").unwrap();
            program.execute(ftx.ptx)
        }
        let mut ctx = budgeted(4);
        ctx.add_function("reenter", reenter);
        // 2 outer iterations, each re-entering for 3 more: 8 > 4.
        assert_eq!(run(&ctx, "[1, 2].all(x, reenter())"), budget_exceeded(4));
        // A single nested execute fits: 1 + 3 = 4.
        assert_eq!(run(&ctx, "[1].all(x, reenter())"), Ok(true.into()));
    }

    #[test]
    fn nested_execute_cannot_discard_the_abort() {
        fn swallow(ftx: &FunctionContext) -> ResolveResult {
            let program = Program::compile("[1, 2, 3].all(x, x > 0)").unwrap();
            let _ = program.execute(ftx.ptx);
            Ok(Value::Bool(true))
        }
        let mut ctx = budgeted(2);
        ctx.add_function("swallow", swallow);
        assert_eq!(run(&ctx, "swallow()"), budget_exceeded(2));
    }

    #[test]
    fn context_resolve_is_budgeted() {
        let ctx = budgeted(1);
        let program = Program::compile("[1, 2].all(x, x > 0)").unwrap();
        assert_eq!(ctx.resolve(program.expression()), budget_exceeded(1));
    }

    #[test]
    fn atomic_bool_interrupts_from_another_thread() {
        let flag = AtomicBool::new(false);
        let list: Vec<i64> = (0..1_000_000).collect();
        let mut ctx = Context::default();
        ctx.add_variable("list", list).unwrap();
        ctx.set_interrupt(&flag);
        // Each iteration runs another `all` over 1000 elements, so a full run takes seconds.
        let program = Program::compile("list.all(x, list.exists(y, y > x + 1000))").unwrap();
        let result = std::thread::scope(|scope| {
            let flag = &flag;
            scope.spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                flag.store(true, Ordering::Relaxed);
            });
            program.execute(&ctx)
        });
        assert_eq!(result, Err(ExecutionError::Interrupted));
    }

    #[test]
    fn deadline_interrupts() {
        let deadline = Deadline::after(Duration::from_millis(10));
        let list: Vec<i64> = (0..1_000_000).collect();
        let mut ctx = Context::default();
        ctx.add_variable("list", list).unwrap();
        ctx.set_interrupt(&deadline);
        let program = Program::compile("list.all(x, list.exists(y, y > x + 1000))").unwrap();
        assert_eq!(program.execute(&ctx), Err(ExecutionError::Interrupted));
    }

    #[test]
    fn closure_interrupts() {
        let interrupt = || true;
        let mut ctx = Context::default();
        ctx.set_interrupt(&interrupt);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0)"),
            Err(ExecutionError::Interrupted)
        );
        // No comprehension, nothing polled: the expression completes.
        assert_eq!(run(&ctx, "1 + 1"), Ok(2.into()));
    }

    #[test]
    fn interrupt_set_on_a_child_scope_is_honoured() {
        let interrupt = || true;
        let root = Context::default();
        let mut child = root.new_inner_scope();
        child.set_interrupt(&interrupt);
        assert_eq!(
            run(&child, "[1, 2].all(x, x > 0)"),
            Err(ExecutionError::Interrupted)
        );
    }

    #[test]
    fn interrupt_is_not_absorbed_by_logical_or() {
        let interrupt = || true;
        let mut ctx = Context::default();
        ctx.set_interrupt(&interrupt);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0) || true"),
            Err(ExecutionError::Interrupted)
        );
    }

    #[test]
    fn interrupt_check_frequency_gates_polling() {
        let polls = AtomicU64::new(0);
        let interrupt = || {
            polls.fetch_add(1, Ordering::Relaxed);
            false
        };
        let mut env = Env::stdlib();
        env.set_options(RuntimeOptions::default().with_interrupt_check_frequency(10));
        let mut ctx = Context::with_env(Arc::new(env));
        ctx.set_interrupt(&interrupt);
        let list: Vec<i64> = (0..100).collect();
        ctx.add_variable("list", list).unwrap();
        assert_eq!(run(&ctx, "list.all(x, x >= 0)"), Ok(true.into()));
        assert_eq!(polls.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn function_observing_interrupt_is_sticky() {
        fn observe(ftx: &FunctionContext) -> ResolveResult {
            assert!(ftx.is_interrupted());
            Ok(Value::Bool(false))
        }
        let interrupt = || true;
        let mut ctx = Context::default();
        ctx.set_interrupt(&interrupt);
        ctx.add_function("observe", observe);
        assert_eq!(run(&ctx, "observe()"), Err(ExecutionError::Interrupted));
    }

    #[test]
    fn function_without_interrupt_is_not_interrupted() {
        fn observe(ftx: &FunctionContext) -> ResolveResult {
            Ok(Value::Bool(ftx.is_interrupted()))
        }
        let mut ctx = Context::default();
        ctx.add_function("observe", observe);
        assert_eq!(run(&ctx, "observe()"), Ok(false.into()));
    }

    #[test]
    fn interrupt_wins_over_budget() {
        let interrupt = || true;
        let mut ctx = budgeted(1);
        ctx.set_interrupt(&interrupt);
        assert_eq!(
            run(&ctx, "[1, 2].all(x, x > 0)"),
            Err(ExecutionError::Interrupted)
        );
    }

    #[test]
    fn frame_records_first_budget_only() {
        let options = RuntimeOptions::default().with_max_iterations(1);
        let frame = Frame::new(&options, None);
        assert!(frame.tick().is_ok());
        assert_eq!(
            frame.tick(),
            Err(ExecutionError::IterationBudgetExceeded { limit: 1 })
        );
        // Sticky: still failing, and finish() overrides a value.
        assert_eq!(
            frame.tick(),
            Err(ExecutionError::IterationBudgetExceeded { limit: 1 })
        );
        assert_eq!(
            frame.finish(Ok(())),
            Err(ExecutionError::IterationBudgetExceeded { limit: 1 })
        );
    }

    #[test]
    fn context_stays_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Context>();
    }
}
