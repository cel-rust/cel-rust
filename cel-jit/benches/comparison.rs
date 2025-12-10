//! Benchmark comparing tree-walking interpreter vs JIT compiled execution.

use cel::context::{Context, VariableResolver};
use cel::{Program, Value};
use cel_jit::CompiledProgram;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

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

/// Benchmark variable access patterns
fn benchmark_variable_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_access");

    // HashMap variable
    {
        let expr = "apple";
        let mut ctx = Context::default();
        ctx.add_variable_from_value("apple", true);

        let interpreted = Program::compile(expr).unwrap();
        let compiled = CompiledProgram::compile(expr).unwrap();

        group.bench_function("interpreted_hashmap", |b| {
            b.iter(|| black_box(interpreted.execute(&ctx)))
        });

        group.bench_function("compiled_hashmap", |b| {
            b.iter(|| black_box(compiled.execute(&ctx)))
        });
    }

    // Variable resolver
    {
        let expr = "banana";
        let mut ctx = Context::default();
        ctx.set_variable_resolver(&Resolver);

        let interpreted = Program::compile(expr).unwrap();
        let compiled = CompiledProgram::compile(expr).unwrap();

        group.bench_function("interpreted_resolver", |b| {
            b.iter(|| black_box(interpreted.execute(&ctx)))
        });

        group.bench_function("compiled_resolver", |b| {
            b.iter(|| black_box(compiled.execute(&ctx)))
        });
    }

    group.finish();
}

/// Benchmark map macro with varying list sizes
fn benchmark_map_macro_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("map_list_scaling");
    let sizes = vec![1, 10, 100, 1000, 10000];

    for size in sizes {
        let list: Vec<i64> = (0..size).collect();
        let expr = "list.map(x, x * 2)";

        let mut ctx = Context::default();
        ctx.add_variable_from_value("list", list);

        let interpreted = Program::compile(expr).unwrap();
        let compiled = CompiledProgram::compile(expr).unwrap();

        group.bench_function(BenchmarkId::new("interpreted", size), |b| {
            b.iter(|| black_box(interpreted.execute(&ctx)))
        });

        group.bench_function(BenchmarkId::new("compiled", size), |b| {
            b.iter(|| black_box(compiled.execute(&ctx)))
        });
    }

    group.finish();
}

/// Benchmark filter macro with varying list sizes
fn benchmark_filter_macro_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("filter_list_scaling");
    let sizes = vec![1, 10, 100, 1000, 10000];

    for size in sizes {
        let list: Vec<i64> = (0..size).collect();
        let expr = "list.filter(x, x % 2 == 0)";

        let mut ctx = Context::default();
        ctx.add_variable_from_value("list", list);

        let interpreted = Program::compile(expr).unwrap();
        let compiled = CompiledProgram::compile(expr).unwrap();

        group.bench_function(BenchmarkId::new("interpreted", size), |b| {
            b.iter(|| black_box(interpreted.execute(&ctx)))
        });

        group.bench_function(BenchmarkId::new("compiled", size), |b| {
            b.iter(|| black_box(compiled.execute(&ctx)))
        });
    }

    group.finish();
}

fn benchmark_simple_arithmetic(c: &mut Criterion) {
    let expr = "1 + 2 * 3 - 4 / 2";
    let ctx = Context::default();

    // Compile both versions
    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("simple_arithmetic");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

fn benchmark_comparison(c: &mut Criterion) {
    let expr = "10 > 5 && 3 < 7 || 1 == 1";
    let ctx = Context::default();

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("comparison");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

fn benchmark_conditional(c: &mut Criterion) {
    let expr = "x > 10 ? x * 2 : x + 5";

    let mut ctx = Context::default();
    ctx.add_variable_from_value("x", 15i64);

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("conditional");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

fn benchmark_nested_expression(c: &mut Criterion) {
    let expr = "((a + b) * (c - d)) / ((e + f) - (g * h))";

    let mut ctx = Context::default();
    ctx.add_variable_from_value("a", 10i64);
    ctx.add_variable_from_value("b", 20i64);
    ctx.add_variable_from_value("c", 30i64);
    ctx.add_variable_from_value("d", 5i64);
    ctx.add_variable_from_value("e", 15i64);
    ctx.add_variable_from_value("f", 25i64);
    ctx.add_variable_from_value("g", 2i64);
    ctx.add_variable_from_value("h", 3i64);

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("nested_expression");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark list comprehension (filter)
fn benchmark_list_filter(c: &mut Criterion) {
    let expr = "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10].filter(x, x > 5)";
    let ctx = Context::default();

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("list_filter");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark list comprehension (map)
fn benchmark_list_map(c: &mut Criterion) {
    let expr = "[1, 2, 3, 4, 5].map(x, x * 2)";
    let ctx = Context::default();

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("list_map");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark all() comprehension
fn benchmark_all(c: &mut Criterion) {
    let expr = "[1, 2, 3, 4, 5].all(x, x > 0)";
    let ctx = Context::default();

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("all_comprehension");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark exists() comprehension
fn benchmark_exists(c: &mut Criterion) {
    let expr = "[1, 2, 3, 4, 5].exists(x, x == 3)";
    let ctx = Context::default();

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("exists_comprehension");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark string operations
fn benchmark_string_ops(c: &mut Criterion) {
    let expr = r#""hello world".startsWith("hello") && "hello world".endsWith("world") && "hello world".contains("o w")"#;
    let ctx = Context::default();

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("string_operations");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark member access chains
fn benchmark_member_access(c: &mut Criterion) {
    let expr = "obj.nested.value + obj.other";

    let mut ctx = Context::default();
    let mut obj: std::collections::HashMap<&str, cel::Value> = std::collections::HashMap::new();
    let mut nested: std::collections::HashMap<&str, i64> = std::collections::HashMap::new();
    nested.insert("value", 42);
    obj.insert("nested", cel::Value::from(nested));
    obj.insert("other", cel::Value::Int(10));
    ctx.add_variable_from_value("obj", obj);

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("member_access");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark custom function calls
fn benchmark_custom_function(c: &mut Criterion) {
    let expr = "add(x, y) + multiply(a, b)";

    let mut ctx = Context::default();
    ctx.add_variable_from_value("x", 10i64);
    ctx.add_variable_from_value("y", 20i64);
    ctx.add_variable_from_value("a", 5i64);
    ctx.add_variable_from_value("b", 3i64);
    ctx.add_function("add", |a: i64, b: i64| a + b);
    ctx.add_function("multiply", |a: i64, b: i64| a * b);

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("custom_function");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark list indexing
fn benchmark_list_indexing(c: &mut Criterion) {
    let expr = "list[0] + list[5] + list[9]";

    let mut ctx = Context::default();
    ctx.add_variable_from_value("list", vec![1i64, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("list_indexing");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark complex real-world expression
fn benchmark_real_world(c: &mut Criterion) {
    // Simulates a policy check expression
    let expr = r#"
        user.age >= 18 &&
        user.role in ["admin", "moderator"] &&
        request.method == "POST" &&
        request.path.startsWith("/api/") &&
        size(request.body) < 1000000
    "#;

    let mut ctx = Context::default();
    let mut user: std::collections::HashMap<&str, cel::Value> = std::collections::HashMap::new();
    user.insert("age", cel::Value::Int(25));
    user.insert("role", cel::Value::String("admin".to_string().into()));
    ctx.add_variable_from_value("user", user);

    let mut request: std::collections::HashMap<&str, cel::Value> = std::collections::HashMap::new();
    request.insert("method", cel::Value::String("POST".to_string().into()));
    request.insert("path", cel::Value::String("/api/users".to_string().into()));
    request.insert("body", cel::Value::String("{}".to_string().into()));
    ctx.add_variable_from_value("request", request);

    let interpreted = Program::compile(expr).unwrap();
    let compiled = CompiledProgram::compile(expr).unwrap();

    let mut group = c.benchmark_group("real_world_policy");

    group.bench_function("interpreted", |b| {
        b.iter(|| black_box(interpreted.execute(&ctx)))
    });

    group.bench_function("compiled", |b| b.iter(|| black_box(compiled.execute(&ctx))));

    group.finish();
}

/// Benchmark with varying list sizes for comprehensions
fn benchmark_comprehension_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("comprehension_scaling");

    for size in [10, 50, 100, 500].iter() {
        let list: Vec<i64> = (1..=*size).collect();
        let expr = "items.filter(x, x % 2 == 0).map(x, x * 2)";

        let mut ctx = Context::default();
        ctx.add_variable_from_value("items", list);

        let interpreted = Program::compile(expr).unwrap();
        let compiled = CompiledProgram::compile(expr).unwrap();

        group.bench_with_input(BenchmarkId::new("interpreted", size), size, |b, _| {
            b.iter(|| black_box(interpreted.execute(&ctx)))
        });

        group.bench_with_input(BenchmarkId::new("compiled", size), size, |b, _| {
            b.iter(|| black_box(compiled.execute(&ctx)))
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_simple_arithmetic,
    benchmark_comparison,
    benchmark_conditional,
    benchmark_nested_expression,
    benchmark_variable_access,
    benchmark_member_access,
    benchmark_list_indexing,
    benchmark_list_filter,
    benchmark_list_map,
    benchmark_all,
    benchmark_exists,
    benchmark_map_macro_scaling,
    benchmark_filter_macro_scaling,
    benchmark_comprehension_scaling,
    benchmark_string_ops,
    benchmark_custom_function,
    benchmark_real_world,
);
criterion_main!(benches);
