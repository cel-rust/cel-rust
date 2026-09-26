use cel::common::types::{CelBool, CelString, STRING_TYPE};
use cel::common::value::{CowVal, Val};
use cel::context::{Context, VariableResolver};
use cel::parser::Parser;
use cel::{Env, ExecutionError, Program, Value};
use criterion::{black_box, criterion_group, BenchmarkId, Criterion};
use std::collections::HashMap;
use std::sync::Arc;

const EXPRESSIONS: [(&str, &str); 34] = [
    ("ternary_1", "(false || true) ? 1 : 2"),
    ("ternary_2", "(true ? false : true) ? 1 : 2"),
    ("or_1", "false || true"),
    ("and_1", "true && false"),
    ("and_2", "true && (false ? 2 : 3) > 2"),
    ("number", "1"),
    ("construct_list", "[1,2,3]"),
    ("construct_list_1", "[1]"),
    ("construct_list_2", "[a, 2]"),
    ("add_list", "[1,2,3] + [4, 5, 6]"),
    ("list_element", "[1,2,3][1]"),
    ("construct_dict", "{1: 2, '3': '4'}"),
    ("add_string", "'abc' + 'def'"),
    ("mapexpr", "{1 + a: 3}"),
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

struct Resolver {
    truthy: CelBool,
    falsy: CelBool,
}

impl Default for Resolver {
    fn default() -> Self {
        Resolver {
            truthy: CelBool::from(true),
            falsy: CelBool::from(false),
        }
    }
}

impl VariableResolver for Resolver {
    fn resolve<'b>(&'b self, expr: &str) -> Option<CowVal<'b, 'b>> {
        let v: &dyn Val = match expr {
            "fruit" => &self.truthy,
            "carrot" => &self.truthy,
            "orange" => &self.truthy,
            "banana" => &self.falsy,
            _ => return None,
        };
        Some(CowVal::Borrowed(v))
    }
}

pub fn criterion_benchmark(c: &mut Criterion) {
    // https://gist.github.com/rhnvrm/db4567fcd87b2cb8e997999e1366d406
    let mut execution_group = c.benchmark_group("execute");
    for (name, expr) in black_box(&EXPRESSIONS) {
        execution_group.bench_function(BenchmarkId::from_parameter(name), |b| {
            let parser = Parser::default();
            let ast = parser.parse(expr).expect("Parsing failed");
            let resolver = Resolver::default();
            let mut ctx = Context::default();
            ctx.add_variable_from_value("foo", HashMap::from([("bar", 1)]));
            ctx.add_variable_from_value("apple", true);
            ctx.add_variable_from_value("a", 1);
            ctx.set_variable_resolver(&resolver);
            b.iter(|| Value::resolve_val(&ast, &ctx).expect("Eval failed!"))
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
            let parser = Parser::default();
            let ast = parser.parse("list.map(x, x * 2)").expect("Parsing failed");
            let mut ctx = Context::default();
            ctx.add_variable_from_value("list", list);
            b.iter(|| Value::resolve_val(&ast, &ctx).expect("Eval failed!"))
        });
    }
    group.finish();
}

fn starts_with_slash<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let path = args[0]
        .downcast_ref::<CelString>()
        .expect("a string target");
    Ok(CowVal::owned(CelBool::from(path.inner().starts_with('/'))))
}

/// A member overload called on a variable: the call's target is an
/// identifier, so its name, with the function's, could be a qualified
/// function name, e.g. `optional.none()`.
pub fn member_overload_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("execute");
    group.bench_function("member_overload", |b| {
        let mut env = Env::stdlib();
        env.add_member_overload(
            "startsWithSlash",
            "string_starts_with_slash",
            STRING_TYPE,
            vec![],
            starts_with_slash,
        )
        .expect("Must be unique");
        let parser = Parser::default();
        let ast = parser
            .parse("path.startsWithSlash()")
            .expect("Parsing failed");
        let mut ctx = Context::with_env(Arc::new(env));
        ctx.add_variable_as_val("path", Box::new(CelString::from("/some/path")));
        b.iter(|| Value::resolve_val(&ast, &ctx).expect("Eval failed!"))
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark, criterion_benchmark_parsing, map_macro_benchmark, member_overload_benchmark
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
