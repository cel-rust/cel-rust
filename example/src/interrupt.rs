use cel::{Context, Deadline, Env, ExecutionError, Program, RuntimeOptions};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// An expression that takes a long time to evaluate: for every element of the
/// list, it scans the whole list again, and `map` never short-circuits.
const SLOW: &str = "list.map(x, list.exists(y, y > x + size(list)))";

fn main() {
    let program = Program::compile(SLOW).unwrap();
    let list: Vec<i64> = (0..100_000).collect();

    // 1. Bound the amount of work with an iteration budget.
    //
    // The budget is runtime policy, so it lives on the `Env`. It counts every
    // comprehension iteration of an evaluation, nested comprehensions included,
    // and is off (zero) by default.
    let mut env = Env::stdlib();
    env.set_options(RuntimeOptions::default().with_max_iterations(1_000_000));
    let env = Arc::new(env);

    let mut context = Context::with_env(env.clone());
    context.add_variable("list", list.clone()).unwrap();
    let result = program.execute(&context);
    assert_eq!(
        result,
        Err(ExecutionError::IterationBudgetExceeded { limit: 1_000_000 })
    );
    println!("budget: {}", result.unwrap_err());

    // 2. Cancel an evaluation after a deadline.
    //
    // Interrupt handles are per evaluation, so they are set on the `Context`.
    // The interpreter polls the handle while iterating comprehensions.
    let deadline = Deadline::after(Duration::from_millis(50));
    let mut context = Context::default();
    context.add_variable("list", list.clone()).unwrap();
    context.set_interrupt(&deadline);
    let result = program.execute(&context);
    assert_eq!(result, Err(ExecutionError::Interrupted));
    println!("deadline: {}", result.unwrap_err());

    // 3. Cancel an evaluation from another thread.
    //
    // Any `Fn() -> bool + Send + Sync` works as a handle too, which is how a
    // runtime's cancellation token plugs in: `ctx.set_interrupt(&|| token.is_cancelled())`.
    let cancelled = AtomicBool::new(false);
    let mut context = Context::default();
    context.add_variable("list", list).unwrap();
    context.set_interrupt(&cancelled);
    let result = thread::scope(|scope| {
        scope.spawn(|| {
            thread::sleep(Duration::from_millis(50));
            cancelled.store(true, Ordering::Relaxed);
        });
        program.execute(&context)
    });
    assert_eq!(result, Err(ExecutionError::Interrupted));
    println!("cancelled: {}", result.unwrap_err());
}
