use cel::context::{Context, VariableResolver};
use cel::{Program, Value};
use criterion::{black_box, criterion_group, BenchmarkId, Criterion};
use std::collections::HashMap;

const EXPRESSIONS: [(&str, &str); 36] = [
    ("ternary_1", "(1 || 2) ? 1 : 2"),
    ("ternary_2", "(1 ? 2 : 3) ? 1 : 2"),
    ("or_1", "1 || 2"),
    ("and_1", "1 && 2"),
    ("and_2", "1 && (false ? 2 : 3)"),
    ("number", "1"),
    ("construct_list", "[1,2,3]"),
    ("construct_list_1", "[1]"),
    ("construct_list_2", "[1, 2]"),
    ("add_list", "[1,2,3] + [4, 5, 6]"),
    ("list_element", "[1,2,3][1]"),
    ("construct_dict", "{1: 2, '3': '4'}"),
    ("add_string", "'abc' + 'def'"),
    ("list", "[1,2,3, Now, ]"),
    ("mapexpr", "{1 + a: 3}"),
    ("map_merge", "{'a': 1} + {'a': 2, 'b': 3}"),
    ("size_list", "[1].size()"),
    ("size_list_1", "size([1])"),
    ("size_str", "'a'.size()"),
    ("size_str_2", "size('a')"),
    ("size_map", "{1:2}.size()"),
    ("size_map_2", "size({1:2})"),
    ("member", "foo.bar"),
    ("map has", "has(foo.bar.baz)"),
    ("map macro", "[1, 2, 3].map(x, x * 2)"),
    ("filter macro", "[1, 2, 3].filter(x, x > 2)"),
    ("all macro", "[1, 2, 3].all(x, x > 0)"),
    ("all map macro", "{0: 0, 1:1, 2:2}.all(x, x >= 0)"),
    ("max", "max(1, 2, 3)"),
    ("max negative", "max(-1, 0, 1)"),
    ("max float", "max(-1.0, 0.0, 1.0)"),
    ("duration", "duration('1s')"),
    ("timestamp", "timestamp('2023-05-28T00:00:00Z')"), // ("complex", "Account{user_id: 123}.user_id == 123"),
    ("variable resolver", "banana"),
    ("variable hashmap", "apple"),
    ("stress", "true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true && true"),
];

struct Resolver;

impl VariableResolver for Resolver {
    fn resolve(&self, expr: &str) -> Option<Value> {
        const V: Value = Value::Bool(false);
        const NOT_V: Value = Value::Bool(true);
        match expr {
            "fruit" => Some(NOT_V),
            "carrot" => Some(NOT_V),
            "orange" => Some(NOT_V),
            "banana" => Some(V),
            _ => None,
        }
    }
}

pub fn criterion_benchmark(c: &mut Criterion) {
    // https://gist.github.com/rhnvrm/db4567fcd87b2cb8e997999e1366d406
    let mut execution_group = c.benchmark_group("execute");
    for (name, expr) in black_box(&EXPRESSIONS) {
        execution_group.bench_function(BenchmarkId::from_parameter(name), |b| {
            let program = Program::compile(expr).expect("Parsing failed");
            let mut ctx = Context::default();
            ctx.add_variable_from_value("foo", HashMap::from([("bar", 1)]));
            ctx.add_variable_from_value("apple", true);
            ctx.set_variable_resolver(std::sync::Arc::new(Resolver));
            b.iter(|| program.execute(&ctx))
        });
    }
}

pub fn criterion_benchmark_parsing(c: &mut Criterion) {
    let mut parsing_group = c.benchmark_group("parse");
    for (name, expr) in black_box(&EXPRESSIONS) {
        parsing_group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| Program::compile(expr).expect("Parsing failed"))
        });
    }
}

pub fn map_macro_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("map list");
    let sizes = vec![1, 10, 100, 1000, 10000, 100000];

    for size in sizes {
        group.bench_function(format!("map_{size}").as_str(), |b| {
            let list = (0..size).collect::<Vec<_>>();
            let program = Program::compile("list.map(x, x * 2)").unwrap();
            let mut ctx = Context::default();
            ctx.add_variable_from_value("list", list);
            b.iter(|| program.execute(&ctx).unwrap())
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark, criterion_benchmark_parsing, map_macro_benchmark
}

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// This is the following macro expanded:
/// criterion_main!(benches);
/// But expanded manually so that we can keep the dhat profiler in scope until after benchmarks run
fn main() {
    #[cfg(feature = "dhat-heap")]
    let profiler = dhat::Profiler::new_heap();

    benches();
    // If adding new criterion groups, do so here.

    // Dropping the dhat profiler prints information to stderr: https://docs.rs/dhat/latest/dhat/
    // Doing so before the below ensures profiler doesn't measure Criterion's summary code.
    // It still may measure other bits of Criterion during the benchmark, of course..
    #[cfg(feature = "dhat-heap")]
    drop(profiler);

    Criterion::default().configure_from_args().final_summary();
}
