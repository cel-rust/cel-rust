use crate::common::value::CowVal;
use crate::context::Context;
use crate::magic::Arguments;
use crate::resolvers::Resolver;
use crate::{ExecutionError, Value};
use std::cmp::Ordering;

type Result<T> = std::result::Result<T, ExecutionError>;

/// `FunctionContext` is a context object passed to functions when they are called.
///
/// It contains references to the target object (if the function is called as
/// a method), the program context ([`Context`]) which gives functions access
/// to variables, and the arguments to the function call.
///
/// `'context` is the borrow of everything handed to the function for the
/// duration of the call, and `'call` bounds the data the values themselves may
/// borrow (a resolver's `&str`, a context variable). A function that returns
/// one of its arguments, or `this`, borrowed can hand back a
/// [`CowVal<'context, 'call>`](CowVal) without copying it.
#[derive(Clone)]
pub struct FunctionContext<'context, 'call: 'context> {
    pub name: &'context str,
    pub this: Option<CowVal<'context, 'call>>,
    pub ptx: &'context Context<'context, 'call>,
    pub args: Vec<CowVal<'context, 'call>>,
    pub arg_idx: usize,
}

impl<'context, 'call: 'context> FunctionContext<'context, 'call> {
    pub fn new(
        name: &'context str,
        this: Option<CowVal<'context, 'call>>,
        ptx: &'context Context<'context, 'call>,
        args: Vec<CowVal<'context, 'call>>,
    ) -> Self {
        Self {
            name,
            this,
            ptx,
            args,
            arg_idx: 0,
        }
    }

    /// Resolves the given expression using the program's [`Context`].
    pub fn resolve<R>(&self, resolver: R) -> Result<Value>
    where
        R: Resolver,
    {
        resolver.resolve(self)
    }

    /// Returns an execution error for the currently execution function.
    pub fn error<M: ToString>(&self, message: M) -> ExecutionError {
        ExecutionError::function_error(self.name, message)
    }
}

pub fn max(Arguments(args): Arguments) -> Result<Value> {
    // If items is a list of values, then operate on the list
    let items = if args.len() == 1 {
        match &args[0] {
            Value::List(values) => values,
            _ => return Ok(args[0].clone()),
        }
    } else {
        &args
    };

    items
        .iter()
        .skip(1)
        .try_fold(items.first().unwrap_or(&Value::Null), |acc, x| {
            match acc.partial_cmp(x) {
                Some(Ordering::Greater) => Ok(acc),
                Some(_) => Ok(x),
                None => Err(ExecutionError::ValuesNotComparable(acc.clone(), x.clone())),
            }
        })
        .cloned()
}

pub fn min(Arguments(args): Arguments) -> Result<Value> {
    // If items is a list of values, then operate on the list
    let items = if args.len() == 1 {
        match &args[0] {
            Value::List(values) => values,
            _ => return Ok(args[0].clone()),
        }
    } else {
        &args
    };

    items
        .iter()
        .skip(1)
        .try_fold(items.first().unwrap_or(&Value::Null), |acc, x| {
            match acc.partial_cmp(x) {
                Some(Ordering::Less) => Ok(acc),
                Some(_) => Ok(x),
                None => Err(ExecutionError::ValuesNotComparable(acc.clone(), x.clone())),
            }
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use crate::context::Context;
    use crate::tests::test_script;

    fn assert_script(input: &(&str, &str)) {
        assert_eq!(test_script(input.1, None), Ok(true.into()), "{}", input.0);
    }

    fn assert_error(input: &(&str, &str, &str)) {
        assert_eq!(
            test_script(input.1, None).map_err(|e| e.to_string()),
            Err(input.2.to_string()),
            "{}",
            input.0
        );
    }

    #[test]
    fn test_size() {
        [
            ("size of list", "size([1, 2, 3]) == 3"),
            ("size of map", "size({'a': 1, 'b': 2, 'c': 3}) == 3"),
            ("size of string", "size('foo') == 3"),
            ("size of bytes", "size(b'foo') == 3"),
            ("size as a list method", "[1, 2, 3].size() == 3"),
            ("size as a string method", "'foobar'.size() == 6"),
            ("size as a bytes method", "b'foobar'.size() == 6"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_has() {
        let tests = vec![
            ("map has", "has(foo.bar) == true"),
            ("map not has", "has(foo.baz) == false"),
        ];

        for (name, script) in tests {
            let mut ctx = Context::default();
            ctx.add_variable_from_value("foo", std::collections::HashMap::from([("bar", 1)]));
            assert_eq!(test_script(script, Some(ctx)), Ok(true.into()), "{name}");
        }
    }

    #[test]
    fn test_map() {
        [
            ("map list", "[1, 2, 3].map(x, x * 2) == [2, 4, 6]"),
            ("map list 2", "[1, 2, 3].map(y, y + 1) == [2, 3, 4]"),
            (
                "map list filter",
                "[1, 2, 3].map(y, y % 2 == 0, y + 1) == [3]",
            ),
            (
                "nested map",
                "[[1, 2], [2, 3]].map(x, x.map(x, x * 2)) == [[2, 4], [4, 6]]",
            ),
            (
                "map to list",
                r#"{'John': 'smart'}.map(key, key) == ['John']"#,
            ),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_filter() {
        [("filter list", "[1, 2, 3].filter(x, x > 2) == [3]")]
            .iter()
            .for_each(assert_script);
    }

    #[test]
    fn test_all() {
        [
            ("all list #1", "[0, 1, 2].all(x, x >= 0)"),
            ("all list #2", "[0, 1, 2].all(x, x > 0) == false"),
            ("all map", "{0: 0, 1:1, 2:2}.all(x, x >= 0) == true"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_exists() {
        [
            ("exist list #1", "[0, 1, 2].exists(x, x > 0)"),
            ("exist list #2", "[0, 1, 2].exists(x, x == 3) == false"),
            ("exist list #3", "[0, 1, 2, 2].exists(x, x == 2)"),
            ("exist map", "{0: 0, 1:1, 2:2}.exists(x, x > 0)"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_exists_one() {
        [
            ("exist list #1", "[0, 1, 2].exists_one(x, x > 0) == false"),
            ("exist list #2", "[0, 1, 2].exists_one(x, x == 0)"),
            ("exist map", "{0: 0, 1:1, 2:2}.exists_one(x, x == 2)"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_max() {
        [
            ("max single", "max(1) == 1"),
            ("max multiple", "max(1, 2, 3) == 3"),
            ("max negative", "max(-1, 0) == 0"),
            ("max float", "max(-1.0, 0.0) == 0.0"),
            ("max list", "max([1, 2, 3]) == 3"),
            ("max empty list", "max([]) == null"),
            ("max no args", "max() == null"),
        ]
        .iter()
        .for_each(|a| {
            let input: &(&str, &str) = a;
            let mut context = Context::default();
            context.add_function("max", super::max).unwrap();
            let ctx = Some(context);
            let r = test_script(input.1, ctx);
            assert_eq!(r, Ok(true.into()), "{}", input.0);
        });
    }

    #[test]
    fn test_min() {
        [
            ("min single", "min(1) == 1"),
            ("min multiple", "min(1, 2, 3) == 1"),
            ("min negative", "min(-1, 0) == -1"),
            ("min float", "min(-1.0, 0.0) == -1.0"),
            (
                "min float multiple",
                "min(1.61803, 3.1415, 2.71828, 1.41421) == 1.41421",
            ),
            ("min list", "min([1, 2, 3]) == 1"),
            ("min empty list", "min([]) == null"),
            ("min no args", "min() == null"),
        ]
        .iter()
        .for_each(|a| {
            let input: &(&str, &str) = a;
            let mut context = Context::default();
            context.add_function("min", super::min).unwrap();
            let ctx = Some(context);
            let r = test_script(input.1, ctx);
            assert_eq!(r, Ok(true.into()), "{}", input.0);
        });
    }

    #[test]
    fn test_starts_with() {
        [
            ("starts with true", "'foobar'.startsWith('foo') == true"),
            ("starts with false", "'foobar'.startsWith('bar') == false"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_ends_with() {
        [
            ("ends with true", "'foobar'.endsWith('bar') == true"),
            ("ends with false", "'foobar'.endsWith('foo') == false"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn test_timestamp() {
        [(
                "comparison",
                "timestamp('2023-05-29T00:00:00Z') > timestamp('2023-05-28T00:00:00Z')",
            ),
            (
                "comparison",
                "timestamp('2023-05-29T00:00:00Z') < timestamp('2023-05-30T00:00:00Z')",
            ),
            (
                "subtracting duration",
                "timestamp('2023-05-29T00:00:00Z') - duration('24h') == timestamp('2023-05-28T00:00:00Z')",
            ),
            (
                "subtracting date",
                "timestamp('2023-05-29T00:00:00Z') - timestamp('2023-05-28T00:00:00Z') == duration('24h')",
            ),
            (
                "adding duration",
                "timestamp('2023-05-28T00:00:00Z') + duration('24h') == timestamp('2023-05-29T00:00:00Z')",
            ),
            (
                "timestamp string",
                "string(timestamp('2023-05-28T00:00:00Z')) == '2023-05-28T00:00:00+00:00'",
            ),
            (
                "timestamp timestamp",
                "string(timestamp(timestamp('2023-05-28T00:00:00Z'))) == '2023-05-28T00:00:00+00:00'",
            ),
            (
                "timestamp getFullYear",
                "timestamp('2023-05-28T00:00:00Z').getFullYear() == 2023",
            ),
            (
                "timestamp getMonth",
                "timestamp('2023-05-28T00:00:00Z').getMonth() == 4",
            ),
            (
                "timestamp getDayOfMonth",
                "timestamp('2023-05-28T00:00:00Z').getDayOfMonth() == 27",
            ),
            (
                "timestamp getDayOfYear",
                "timestamp('2023-05-28T00:00:00Z').getDayOfYear() == 147",
            ),
            (
                "timestamp getDate",
                "timestamp('2023-05-28T00:00:00Z').getDate() == 28",
            ),
            (
                "timestamp getDayOfWeek",
                "timestamp('2023-05-28T00:00:00Z').getDayOfWeek() == 0",
            ),
            (
                "timestamp getHours",
                "timestamp('2023-05-28T02:00:00Z').getHours() == 2",
            ),
            (
                "timestamp getMinutes",
                " timestamp('2023-05-28T00:05:00Z').getMinutes() == 5",
            ),
            (
                "timestamp getSeconds",
                "timestamp('2023-05-28T00:00:06Z').getSeconds() == 6",
            ),
            (
                "timestamp getMilliseconds",
                "timestamp('2023-05-28T00:00:42.123Z').getMilliseconds() == 123",
            ),
        ]
        .iter()
        .for_each(assert_script);

        [
            (
                "timestamp out of range",
                "timestamp('0000-01-00T00:00:00Z')",
                "Error executing function 'timestamp': input is out of range",
            ),
            (
                "timestamp out of range",
                "timestamp('9999-12-32T23:59:59.999999999Z')",
                "Error executing function 'timestamp': input is out of range",
            ),
            (
                "timestamp overflow",
                "timestamp('9999-12-31T23:59:59Z') + duration('1s')",
                "Overflow from binary operator 'add': Timestamp(9999-12-31T23:59:59+00:00), Duration(TimeDelta { secs: 1, nanos: 0 })",
            ),
            (
                "timestamp underflow",
                "timestamp('0001-01-01T00:00:00Z') - duration('1s')",
                "Overflow from binary operator 'sub': Timestamp(0001-01-01T00:00:00+00:00), Duration(TimeDelta { secs: 1, nanos: 0 })",
            ),
            (
                "timestamp underflow",
                "timestamp('0001-01-01T00:00:00Z') + duration('-1s')",
                "Overflow from binary operator 'add': Timestamp(0001-01-01T00:00:00+00:00), Duration(TimeDelta { secs: -1, nanos: 0 })",
            )
        ]
        .iter()
        .for_each(assert_error)
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn test_duration() {
        [
            ("duration equal 1", "duration('1s') == duration('1000ms')"),
            ("duration equal 2", "duration('1m') == duration('60s')"),
            ("duration equal 3", "duration('1h') == duration('60m')"),
            ("duration comparison 1", "duration('1m') > duration('1s')"),
            ("duration comparison 2", "duration('1m') < duration('1h')"),
            (
                "duration subtraction",
                "duration('1h') - duration('1m') == duration('59m')",
            ),
            (
                "duration addition",
                "duration('1h') + duration('1m') == duration('1h1m')",
            ),
            ("duration getHours", "duration('2h30m45s').getHours() == 2"),
            (
                "duration getMinutes",
                "duration('2h30m45s').getMinutes() == 150",
            ),
            (
                "duration getSeconds",
                "duration('2h30m45s').getSeconds() == 9045",
            ),
            (
                "duration getMilliseconds",
                "duration('1s500ms').getMilliseconds() == 1500",
            ),
            (
                "duration getHours overflow",
                "duration('25h').getHours() == 25",
            ),
            (
                "duration getMinutes overflow",
                "duration('90m').getMinutes() == 90",
            ),
            (
                "duration getSeconds overflow",
                "duration('90s').getSeconds() == 90",
            ),
            (
                "duration getSeconds overflow",
                "duration(duration('13s')).getSeconds() == 13",
            ),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn test_timestamp_variable() {
        let mut context = Context::default();
        let ts: chrono::DateTime<chrono::FixedOffset> =
            chrono::DateTime::parse_from_rfc3339("2023-05-29T00:00:00Z").unwrap();
        context
            .add_variable("ts", crate::Value::Timestamp(ts))
            .unwrap();

        let program = crate::Program::compile("ts == timestamp('2023-05-29T00:00:00Z')").unwrap();
        let result = program.execute(&context).unwrap();
        assert_eq!(result, true.into());
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn test_chrono_string() {
        [
            ("duration", "string(duration('1h30m')) == '1h30m0s'"),
            (
                "timestamp",
                "string(timestamp('2023-05-29T00:00:00Z')) == '2023-05-29T00:00:00+00:00'",
            ),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_contains() {
        let tests = vec![("string", "'foobar'.contains('bar') == true")];

        for (name, script) in tests {
            assert_eq!(test_script(script, None), Ok(true.into()), "{name}");
        }
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_matches() {
        let tests = vec![
            ("string", "'foobar'.matches('^[a-zA-Z]*$') == true"),
            (
                "map",
                "{'1': 'abc', '2': 'def', '3': 'ghi'}.all(key, key.matches('^[a-zA-Z]*$')) == false",
            ),
        ];

        for (name, script) in tests {
            assert_eq!(
                test_script(script, None),
                Ok(true.into()),
                ".matches failed for '{name}'"
            );
        }
    }

    #[cfg(feature = "regex")]
    #[test]
    fn test_matches_err() {
        assert_eq!(
            test_script(
                "'foobar'.matches('(foo') == true", None),
            Err(
                crate::ExecutionError::FunctionError {
                    function: "matches".to_string(),
                    message: "'(foo' not a valid regex:\nregex parse error:\n    (foo\n    ^\nerror: unclosed group".to_string()
                }
            )
        );
    }

    #[test]
    fn test_string() {
        [
            ("string", "string('foo') == 'foo'"),
            ("int", "string(10) == '10'"),
            ("float", "string(10.5) == '10.5'"),
            ("bytes", "string(b'foo') == 'foo'"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_bytes() {
        [
            ("string", "bytes('abc') == b'abc'"),
            ("bytes", "bytes('abc') == b'\\x61b\\x63'"),
            ("bytes_to_bytes", "bytes(b'abc') == b'\\x61b\\x63'"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_double() {
        [
            ("string", "double('10') == 10.0"),
            ("int", "double(10) == 10.0"),
            ("double", "double(10.0) == 10.0"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_uint() {
        [
            ("uint", "uint(10u) == 10u"),
            ("int", "uint(10) == 10u"),
            ("string", "uint('10') == 10u"),
            ("double", "uint(10.5) == 10u"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn test_int() {
        [
            ("string", "int('10') == 10"),
            ("int", "int(10) == 10"),
            ("uint", "int(10u) == 10"),
            ("double", "int(10.5) == 10"),
        ]
        .iter()
        .for_each(assert_script);
    }

    #[test]
    fn no_bool_coercion() {
        [
            (
                "string || bool",
                "'' || false",
                "found no matching overload for '_||_' applied to '(string, bool)'",
            ),
            (
                "int || bool",
                "1 || false",
                "found no matching overload for '_||_' applied to '(int, bool)'",
            ),
            (
                "uint || bool",
                "1u || false",
                "found no matching overload for '_||_' applied to '(uint, bool)'",
            ),
            (
                "double || bool",
                "0.1|| false",
                "found no matching overload for '_||_' applied to '(double, bool)'",
            ),
            (
                "list || bool",
                "[] || false",
                "found no matching overload for '_||_' applied to '(list, bool)'",
            ),
            (
                "map || bool",
                "{} || false",
                "found no matching overload for '_||_' applied to '(map, bool)'",
            ),
            (
                "null || bool",
                "null || false",
                "found no matching overload for '_||_' applied to '(null_type, bool)'",
            ),
        ]
        .iter()
        .for_each(assert_error)
    }
}
