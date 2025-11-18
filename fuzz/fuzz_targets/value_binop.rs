#![no_main]

use cel::Value;
use libfuzzer_sys::fuzz_target;
use std::hint::black_box;

#[derive(Debug, arbitrary::Arbitrary)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Cmp,
}

#[derive(Debug)]
struct Input {
    op: BinOp,
    lhs: Value,
    rhs: Value,
}

::std::thread_local! {# [allow (non_upper_case_globals )]static RECURSIVE_COUNT_Value : :: core :: cell :: Cell < u32 > = :: core :: cell :: Cell :: new (0 ); }
#[automatically_derived]
impl<'arbitrary> arbitrary::Arbitrary<'arbitrary> for Input {
    fn arbitrary(u: &mut arbitrary::Unstructured<'arbitrary>) -> arbitrary::Result<Self> {
        let op = u.arbitrary::<BinOp>()?;
        let lhs = arbitrary_value(u)?;
        let rhs = arbitrary_value(u)?;
        Ok(Self { op, lhs, rhs })
    }
}

fn arbitrary_value(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Value> {
    let guard_against_recursion = u.is_empty();
    if guard_against_recursion {
        RECURSIVE_COUNT_Value.with(|count| {
            if count.get() > 0 {
                return Err(arbitrary::Error::NotEnoughData);
            }
            count.set(count.get() + 1);
            Ok(())
        })?;
    }
    let result = (|| {
        Ok(
            match (u64::from(<u32 as arbitrary::Arbitrary>::arbitrary(u)?) * 11u64) >> 32 {
                0u64 => Value::List(arbitrary::Arbitrary::arbitrary(u)?),
                1u64 => Value::Map(arbitrary::Arbitrary::arbitrary(u)?),
                2u64 => Value::Int(arbitrary::Arbitrary::arbitrary(u)?),
                3u64 => Value::UInt(arbitrary::Arbitrary::arbitrary(u)?),
                4u64 => Value::Float(arbitrary::Arbitrary::arbitrary(u)?),
                5u64 => Value::String(arbitrary::Arbitrary::arbitrary(u)?),
                6u64 => Value::Bytes(arbitrary::Arbitrary::arbitrary(u)?),
                7u64 => Value::Bool(arbitrary::Arbitrary::arbitrary(u)?),
                8u64 => {
                    let delta = arbitrary::Arbitrary::arbitrary(u)?;
                    let duration = arbitrary::Arbitrary::arbitrary(u)?;
                    Value::Duration(duration)
                }
                9u64 => Value::Timestamp(arbitrary::Arbitrary::arbitrary(u)?),
                10u64 => Value::Null,
                _ => unreachable!(),
            },
        )
    })();
    if guard_against_recursion {
        RECURSIVE_COUNT_Value.with(|count| {
            count.set(count.get() - 1);
        });
    }
    result
}

// Ensure that the binary operators on `Value` do not panic,
// c.f. https://github.com/cel-rust/cel-rust/pull/145.
fuzz_target!(|input: Input| {
    match input.op {
        BinOp::Add => _ = black_box(input.lhs + input.rhs),
        BinOp::Sub => _ = black_box(input.lhs - input.rhs),
        BinOp::Mul => _ = black_box(input.lhs * input.rhs),
        BinOp::Div => _ = black_box(input.lhs / input.rhs),
        BinOp::Rem => _ = black_box(input.lhs % input.rhs),
        BinOp::Eq => _ = black_box(input.lhs == input.rhs),
        BinOp::Cmp => _ = black_box(input.lhs.partial_cmp(&input.rhs)),
    }
});
