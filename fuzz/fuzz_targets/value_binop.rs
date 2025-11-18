#![no_main]

use cel::objects::{Key, Map};
use cel::Value;
use chrono::TimeZone;
use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;

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

::std::thread_local! {# [allow (non_upper_case_globals )]static RECURSIVE_COUNT_Value : core::cell::Cell<u32> = const {core::cell::Cell::new(0)}; }
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

    let result = Ok(
        match (u64::from(<u32 as arbitrary::Arbitrary>::arbitrary(u)?) * 11u64) >> 32 {
            0u64 => {
                let length = <u8 as arbitrary::Arbitrary>::arbitrary(u)?;
                let mut list = Vec::with_capacity(length as usize);
                for _ in 0..length {
                    list.push(arbitrary_value(u)?);
                }
                Value::List(Arc::new(list))
            }
            1u64 => {
                let length = <u8 as arbitrary::Arbitrary>::arbitrary(u)?;
                let mut map = HashMap::with_capacity(length as usize);
                for _ in 0..length {
                    map.insert(arbitrary_key(u)?, arbitrary_value(u)?);
                }
                Value::Map(Map { map: Arc::new(map) })
            }
            2u64 => Value::Int(arbitrary::Arbitrary::arbitrary(u)?),
            3u64 => Value::UInt(arbitrary::Arbitrary::arbitrary(u)?),
            4u64 => Value::Float(arbitrary::Arbitrary::arbitrary(u)?),
            5u64 => Value::String(arbitrary::Arbitrary::arbitrary(u)?),
            6u64 => Value::Bytes(arbitrary::Arbitrary::arbitrary(u)?),
            7u64 => Value::Bool(arbitrary::Arbitrary::arbitrary(u)?),
            8u64 => Value::Duration(chrono::Duration::nanoseconds(
                arbitrary::Arbitrary::arbitrary(u)?,
            )),
            9u64 => Value::Timestamp(
                chrono::Utc
                    .timestamp_nanos(arbitrary::Arbitrary::arbitrary(u)?)
                    .into(),
            ),
            10u64 => Value::Null,
            _ => unreachable!(),
        },
    );

    if guard_against_recursion {
        RECURSIVE_COUNT_Value.with(|count| {
            count.set(count.get() - 1);
        });
    }
    result
}

fn arbitrary_key(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Key> {
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

    let result = Ok(
        match (u64::from(<u32 as arbitrary::Arbitrary>::arbitrary(u)?) * 4u64) >> 32 {
            0u64 => Key::Int(arbitrary::Arbitrary::arbitrary(u)?),
            1u64 => Key::Uint(arbitrary::Arbitrary::arbitrary(u)?),
            2u64 => Key::Bool(arbitrary::Arbitrary::arbitrary(u)?),
            3u64 => Key::String(Arc::new(arbitrary::Arbitrary::arbitrary(u)?)),
            _ => unreachable!(),
        },
    );

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
