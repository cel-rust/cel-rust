//! The iteration budget: a cap on how much work one evaluation may do.
//!
//! A CEL expression comes with no bound on its own cost: `list.all(..)` does as
//! much work as `list` is long, and comprehensions nest. When the expression
//! (or the data it runs over) is not fully under your control, an iteration
//! budget turns "could run for a very long time" into an error you can handle.
//!
//! The budget is *runtime policy*, so it is configured on the [`Env`] through
//! [`RuntimeOptions`], and it counts comprehension iterations (`all`, `exists`,
//! `map`, `filter`, ...): one per element visited, nested comprehensions
//! included. Everything else an expression does is free. When an evaluation
//! goes over, it fails with [`ExecutionError::IterationBudgetExceeded`].
//!
//! Run with `cargo run -p example --bin example-budget`.
use cel::{Context, Env, ExecutionError, Program, ResolveResult, RuntimeOptions};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// An environment that caps every evaluation at `max_iterations`.
fn env_with_budget(max_iterations: u64) -> Arc<Env> {
    let mut env = Env::stdlib();
    env.set_options(RuntimeOptions::default().with_max_iterations(max_iterations));
    Arc::new(env)
}

/// Compiles and runs `source`, with `list` bound to `1..=len`.
fn run(context: &mut Context, source: &str, len: i64) -> ResolveResult {
    let list: Vec<i64> = (1..=len).collect();
    context.add_variable("list", list).unwrap();
    Program::compile(source).unwrap().execute(context)
}

fn exceeded(limit: u64) -> ResolveResult {
    Err(ExecutionError::IterationBudgetExceeded { limit })
}

fn main() {
    // 1. There is no budget unless you set one.
    //
    // Zero means unlimited, which is what `Env::stdlib()` gives you.
    assert_eq!(Env::stdlib().options().max_iterations(), 0);
    let mut context = Context::default();
    let result = run(&mut context, "list.all(x, x > 0)", 20_000);
    assert_eq!(result, Ok(true.into()));
    println!("default: 20000 iterations, no limit, evaluated fine");

    // 2. `with_max_iterations` caps the total number of iterations.
    //
    // The limit is exact: a budget of N allows N iterations and fails on the
    // N+1st. The error carries the limit that was hit.
    let env = env_with_budget(5);
    assert_eq!(env.options().max_iterations(), 5);
    let mut context = Context::with_env(env);
    assert_eq!(run(&mut context, "list.all(x, x > 0)", 5), Ok(true.into()));
    let result = run(&mut context, "list.all(x, x > 0)", 6);
    assert_eq!(result, exceeded(5));
    println!("limit 5: 6 iterations -> {}", result.unwrap_err());

    // 3. The budget is spent by the whole evaluation, nested comprehensions
    //    included.
    //
    // The outer `map` runs 3 iterations, and each one runs the inner `all` for
    // 2 more: 3 + 3 * 2 = 9. Cost multiplies as comprehensions nest, which is
    // exactly what makes it worth bounding. Plain arithmetic and function calls
    // cost nothing, so this last program fits a budget of 1 with room to spare.
    let nested = "list.map(x, [10, 20].all(y, y > x))";
    let mut context = Context::with_env(env_with_budget(9));
    assert!(run(&mut context, nested, 3).is_ok());
    let mut context = Context::with_env(env_with_budget(8));
    assert_eq!(run(&mut context, nested, 3), exceeded(8));
    let mut context = Context::with_env(env_with_budget(1));
    let cheap = "1 + 2 * 3 == 7 && size(list) == 3";
    assert_eq!(run(&mut context, cheap, 3), Ok(true.into()));
    println!("nested: 3 + 3 * 2 = 9 iterations; arithmetic is free");

    // 4. The budget belongs to a single evaluation.
    //
    // It is not spent down across calls: every `execute` starts from zero, so a
    // budget that exactly fits one evaluation fits every later one too, and
    // contexts built from one shared `Env` each get their own count.
    let env = env_with_budget(9);
    let mut first = Context::with_env(env.clone());
    let mut second = Context::with_env(env);
    for _ in 0..3 {
        assert!(run(&mut first, nested, 3).is_ok());
        assert!(run(&mut second, nested, 3).is_ok());
    }
    println!("per evaluation: 6 runs of a 9 iteration program, each within a budget of 9");

    // 5. It is a guard against inputs that grow.
    //
    // The same expression is fine on a small list and rejected on a big one,
    // without you having to look at the data first.
    let mut context = Context::with_env(env_with_budget(1_000));
    assert!(run(&mut context, "list.all(x, x > 0)", 1_000).is_ok());
    let result = run(&mut context, "list.all(x, x > 0)", 1_001);
    assert_eq!(result, exceeded(1_000));
    println!(
        "input size: 1000 elements fit, 1001 -> {}",
        result.unwrap_err()
    );

    // 6. Running out of budget is fatal, not an ordinary error.
    //
    // CEL lets `||` and `&&` absorb an error when the other side settles the
    // answer: `1 / 0 == 1 || true` is `true`. A budget overrun is not absorbed,
    // or a caller could hide the overrun behind `|| true` and get a normal
    // looking answer from an evaluation that was cut short.
    let mut context = Context::with_env(env_with_budget(2));
    let ordinary = "1 / 0 == 1 || true";
    assert_eq!(run(&mut context, ordinary, 5), Ok(true.into()));
    let overrun = "list.all(x, x > 0) || true";
    assert_eq!(run(&mut context, overrun, 5), exceeded(2));
    println!("fatal: `{ordinary}` is true, `{overrun}` is an error");

    // 7. The budget composes with the interrupt poll frequency.
    //
    // `RuntimeOptions` is a builder. Besides the budget, it sets how often an
    // interrupt handle (see the `interrupt` example) is polled: every iteration
    // by default, which is precise but not free for a handle that is expensive
    // to ask. Polling every 100th iteration is 100 times fewer calls, and has
    // no effect on the budget, which stays exact.
    let options = RuntimeOptions::default()
        .with_max_iterations(2_000)
        .with_interrupt_check_frequency(100);
    assert_eq!(options.max_iterations(), 2_000);
    assert_eq!(options.interrupt_check_frequency(), 100);
    // (zero is clamped: choosing a frequency never turns interruption off)
    assert_eq!(
        RuntimeOptions::default()
            .with_interrupt_check_frequency(0)
            .interrupt_check_frequency(),
        1
    );

    let polls = AtomicU64::new(0);
    let counting_handle = || {
        polls.fetch_add(1, Ordering::Relaxed);
        false // never interrupts: we only want to count the polls
    };
    let mut env = Env::stdlib();
    env.set_options(options);
    let mut context = Context::with_env(Arc::new(env));
    context.set_interrupt(&counting_handle);
    assert_eq!(
        run(&mut context, "list.all(x, x > 0)", 1_000),
        Ok(true.into())
    );
    assert_eq!(polls.load(Ordering::Relaxed), 10);
    println!("poll frequency: 1000 iterations, handle polled 10 times");
    assert_eq!(
        run(&mut context, "list.all(x, x > 0)", 2_001),
        exceeded(2_000)
    );
}
