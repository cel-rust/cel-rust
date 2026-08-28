use cel::parser::{Parser, PrattParser};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

struct BenchTestInfo {
    input: String,
    expect_err: bool,
}

struct BenchCategory {
    name: &'static str,
    cases: Vec<BenchTestInfo>,
}

fn bench_categories() -> Vec<BenchCategory> {
    vec![
        // Simple: common, representative CEL expressions covering basic syntax, operators, calls, and literals
        BenchCategory {
            name: "Simple",
            cases: vec![
                BenchTestInfo {
                    input: "x * 2 + y / 3".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "foo.bar.baz(1, 2, \"abc\")".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "a > 5 && b < 10 || c == \"xyz\"".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "x ? y : z".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "{\"foo\": 1, \"bar\": [2, 3]}".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "a[b]".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "a.b.c".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "a.`b-c`".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "\"\\a\\b\\f\\n\\r\\t\\v'\\\"\\\\ Legal escapes \\u2764\"".to_string(),
                    expect_err: false,
                },
            ],
        },
        // Complex: expressions with deep chaining, nesting, precedence, and complex structures
        BenchCategory {
            name: "Complex",
            cases: vec![
                BenchTestInfo {
                    input: "a".to_string() + &" + a".repeat(49),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "a".to_string() + &" || a".repeat(49),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "a".to_string() + &".f".repeat(49),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "(".repeat(20) + "a" + &")".repeat(20),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "SomeMessage{foo: 5, bar: \"xyz\"}".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "1 + 2 * 3 - 1 / 2 == 6 % 1".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "[] + [1, 2, 3] + [4]".to_string(),
                    expect_err: false,
                },
            ],
        },
        // Macros: standard and receiver comprehension macros, optional syntax traversal
        BenchCategory {
            name: "Macros",
            cases: vec![
                BenchTestInfo {
                    input: "has(m.f)".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "[1, 2, 3].all(x, x > 0)".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "m.map(v, v * 2)".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "m.filter(v, v > 0)".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "m.exists_one(v, v == 1)".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "x.filter(y, y.exists(z, has(z.a)))".to_string(),
                    expect_err: false,
                },
                BenchTestInfo {
                    input: "a.?b[?0] && a[?c]".to_string(),
                    expect_err: false,
                },
            ],
        },
        // Errors: representative syntax errors, invalid tokens, keywords, and unclosed delimiters
        BenchCategory {
            name: "Errors",
            cases: vec![
                BenchTestInfo {
                    input: "x * 2 + y /".to_string(),
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "foo.bar.baz(1, 2, \"abc\"".to_string(),
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "a > 5 && && b < 10".to_string(),
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "{\"foo\": 1, \"bar\": [2, 3".to_string(),
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "1 + $".to_string(),
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "break".to_string(),
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "\"\\xFh\"".to_string(),
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "a".to_string() + &" + a".repeat(49) + " +",
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "(".repeat(20) + "a",
                    expect_err: true,
                },
                BenchTestInfo {
                    input: "f(*".to_string() + &", *".repeat(9) + ")",
                    expect_err: true,
                },
            ],
        },
    ]
}

fn new_antlr_parser() -> Parser {
    Parser::default()
        .enable_optional_syntax(true)
        .max_recursion_depth(512)
}

fn new_pratt_parser() -> PrattParser {
    PrattParser::default()
        .enable_optional_syntax(true)
        .enable_ident_escape_syntax(true)
        .max_recursion_depth(512)
}

pub fn benchmark_by_category(c: &mut Criterion) {
    let categories = bench_categories();

    for mode in ["antlr", "pratt"] {
        let mut group = c.benchmark_group(format!("by_category/{mode}"));
        for cat in &categories {
            group.bench_function(BenchmarkId::from_parameter(cat.name), |b| {
                b.iter(|| {
                    for tc in &cat.cases {
                        let is_err = if mode == "antlr" {
                            let parser = new_antlr_parser();
                            parser.parse(black_box(&tc.input)).is_err()
                        } else {
                            let parser = new_pratt_parser();
                            parser.parse(black_box(&tc.input)).is_err()
                        };
                        assert_eq!(is_err, tc.expect_err, "Failed test case: {}", tc.input);
                    }
                });
            });
        }
        group.finish();
    }
}

pub fn benchmark_by_category_comparison(c: &mut Criterion) {
    let categories = bench_categories();

    for cat in &categories {
        let mut group = c.benchmark_group(cat.name);

        group.bench_function("antlr", |b| {
            b.iter(|| {
                for tc in &cat.cases {
                    let res = new_antlr_parser().parse(black_box(&tc.input));
                    assert_eq!(res.is_err(), tc.expect_err, "Failed: {}", tc.input);
                }
            });
        });

        group.bench_function("pratt", |b| {
            b.iter(|| {
                for tc in &cat.cases {
                    let res = new_pratt_parser().parse(black_box(&tc.input));
                    assert_eq!(res.is_err(), tc.expect_err, "Failed: {}", tc.input);
                }
            });
        });

        group.finish();
    }
}

criterion_group!(
    benches,
    benchmark_by_category,
    benchmark_by_category_comparison
);
criterion_main!(benches);
