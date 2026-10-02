//! The strings extension library: member functions on `string` (and
//! `list(string).join`), and `strings.quote`, following cel-go's
//! `ext.Strings()`.
//!
//! Indices are in code points, not bytes, as in the rest of CEL.

mod format;

use super::{arg, iterable};
use crate::common::types::{CelInt, CelList, CelString, LIST_TYPE, STRING_TYPE};
use crate::common::value::{CowVal, Val};
use crate::{DeclarationError, Env, ExecutionError};

/// Registers the strings extension's overloads on `env`.
pub fn extension(env: &mut Env) -> Result<(), DeclarationError> {
    crate::add_member_overload!(env, fn char_at: (CelString, CelInt) -> Result<CelString>)?;
    crate::add_member_overload!(env, fn index_of: (CelString, CelString) -> CelInt)?;
    crate::add_member_overload!(env, fn index_of_offset: (CelString, CelString, CelInt) -> Result<CelInt>,
        name = "indexOf")?;
    crate::add_member_overload!(env, fn last_index_of: (CelString, CelString) -> CelInt)?;
    crate::add_member_overload!(env, fn last_index_of_offset: (CelString, CelString, CelInt) -> Result<CelInt>,
        name = "lastIndexOf")?;
    // Registered by hand, as the macro would downcast the receiver to a
    // `CelList`: `join` takes any `list`, like the stdlib's list overloads.
    env.add_member_overload("join", "list.join()", LIST_TYPE, vec![], join)?;
    env.add_member_overload(
        "join",
        "list.join(string)",
        LIST_TYPE,
        vec![STRING_TYPE],
        join_sep,
    )?;
    crate::add_member_overload!(env, fn lower_ascii: (CelString) -> CelString)?;
    crate::add_member_overload!(env, fn upper_ascii: (CelString) -> CelString)?;
    crate::add_member_overload!(env, fn trim: (CelString) -> CelString)?;
    crate::add_member_overload!(env, fn replace: (CelString, CelString, CelString) -> CelString)?;
    crate::add_member_overload!(env, fn replace_n: (CelString, CelString, CelString, CelInt) -> CelString,
        name = "replace")?;
    crate::add_member_overload!(env, fn split: (CelString, CelString) -> CelList)?;
    crate::add_member_overload!(env, fn split_n: (CelString, CelString, CelInt) -> CelList,
        name = "split")?;
    crate::add_member_overload!(env, fn substring: (CelString, CelInt) -> Result<CelString>)?;
    crate::add_member_overload!(env, fn substring_range: (CelString, CelInt, CelInt) -> Result<CelString>,
        name = "substring")?;
    // By hand, as `join` is: `format` takes any `list`.
    env.add_member_overload(
        "format",
        "string.format(list)",
        STRING_TYPE,
        vec![LIST_TYPE],
        format::format,
    )?;
    crate::add_overload!(env, fn quote: (CelString) -> CelString, name = "strings.quote")?;
    crate::add_member_overload!(env, fn reverse: (CelString) -> CelString)?;
    Ok(())
}

fn out_of_range(function: &str, idx: i64) -> ExecutionError {
    ExecutionError::function_error(function, format!("index out of range: {idx}"))
}

fn char_at(this: &CelString<'_>, idx: &CelInt) -> Result<CelString<'static>, ExecutionError> {
    let idx = *idx.inner();
    let len = this.chars().count() as i64;
    if idx < 0 || idx > len {
        return Err(out_of_range("charAt", idx));
    }
    Ok(this
        .chars()
        .nth(idx as usize)
        .map(|c| c.to_string())
        .unwrap_or_default()
        .into())
}

/// The code-point index of the first occurrence of `needle` in `runes`, at or
/// after `offset`.
fn find_runes(runes: &[char], needle: &[char], offset: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(offset);
    }
    runes[offset..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + offset)
}

/// The code-point index of the last occurrence of `needle` in `runes` that
/// starts at or before `offset`.
fn rfind_runes(runes: &[char], needle: &[char], offset: usize) -> Option<usize> {
    if needle.len() > runes.len() {
        return None;
    }
    let last = offset.min(runes.len() - needle.len());
    (0..=last)
        .rev()
        .find(|&i| runes[i..i + needle.len()] == *needle)
}

fn index_of(this: &CelString<'_>, needle: &CelString<'_>) -> CelInt {
    let runes: Vec<char> = this.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    find_runes(&runes, &needle, 0)
        .map_or(-1, |i| i as i64)
        .into()
}

fn index_of_offset(
    this: &CelString<'_>,
    needle: &CelString<'_>,
    offset: &CelInt,
) -> Result<CelInt, ExecutionError> {
    let offset = *offset.inner();
    let runes: Vec<char> = this.chars().collect();
    if offset < 0 {
        return Err(out_of_range("indexOf", offset));
    }
    if needle.is_empty() {
        return Ok(offset.min(runes.len() as i64).into());
    }
    if offset >= runes.len() as i64 {
        return Err(out_of_range("indexOf", offset));
    }
    let needle: Vec<char> = needle.chars().collect();
    Ok(find_runes(&runes, &needle, offset as usize)
        .map_or(-1, |i| i as i64)
        .into())
}

fn last_index_of(this: &CelString<'_>, needle: &CelString<'_>) -> CelInt {
    let runes: Vec<char> = this.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    rfind_runes(&runes, &needle, runes.len())
        .map_or(-1, |i| i as i64)
        .into()
}

fn last_index_of_offset(
    this: &CelString<'_>,
    needle: &CelString<'_>,
    offset: &CelInt,
) -> Result<CelInt, ExecutionError> {
    let offset = *offset.inner();
    let runes: Vec<char> = this.chars().collect();
    if offset < 0 {
        return Err(out_of_range("lastIndexOf", offset));
    }
    if needle.is_empty() {
        return Ok(offset.min(runes.len() as i64).into());
    }
    if offset >= runes.len() as i64 {
        return Err(out_of_range("lastIndexOf", offset));
    }
    let needle: Vec<char> = needle.chars().collect();
    Ok(rfind_runes(&runes, &needle, offset as usize)
        .map_or(-1, |i| i as i64)
        .into())
}

fn join<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match args.as_slice() {
        [list] => join_with(list.as_ref(), ""),
        _ => Err(ExecutionError::invalid_argument_count(1, args.len())),
    }
}

fn join_sep<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    match args.as_slice() {
        [list, sep] => join_with(list.as_ref(), arg::<CelString>(sep.as_ref())?.inner()),
        _ => Err(ExecutionError::invalid_argument_count(2, args.len())),
    }
}

/// Concatenates the strings of any iterable `list`, `sep` between each.
fn join_with<'b, 'v>(list: &dyn Val, sep: &str) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let mut items = iterable(list)?.iter();
    let mut parts = Vec::new();
    while let Some(item) = items.next() {
        parts.push(arg::<CelString>(item)?.inner());
    }
    Ok(CowVal::owned(CelString::from(parts.join(sep))))
}

fn lower_ascii(this: &CelString<'_>) -> CelString<'static> {
    this.to_ascii_lowercase().into()
}

fn upper_ascii(this: &CelString<'_>) -> CelString<'static> {
    this.to_ascii_uppercase().into()
}

fn trim(this: &CelString<'_>) -> CelString<'static> {
    this.trim().to_owned().into()
}

fn replace(this: &CelString<'_>, from: &CelString<'_>, to: &CelString<'_>) -> CelString<'static> {
    this.replace(from.inner(), to.inner()).into()
}

/// Replaces the first `n` occurrences, or all of them when `n` is negative.
fn replace_n(
    this: &CelString<'_>,
    from: &CelString<'_>,
    to: &CelString<'_>,
    n: &CelInt,
) -> CelString<'static> {
    match usize::try_from(*n.inner()) {
        Ok(n) => this.replacen(from.inner(), to.inner(), n).into(),
        Err(_) => replace(this, from, to),
    }
}

fn split(this: &CelString<'_>, sep: &CelString<'_>) -> CelList<'static> {
    split_n(this, sep, &CelInt::from(-1))
}

/// Splits into at most `n` parts, the last holding the unsplit remainder: none
/// when `n` is zero, all of them when it is negative. An empty separator splits
/// between code points.
fn split_n(this: &CelString<'_>, sep: &CelString<'_>, n: &CelInt) -> CelList<'static> {
    let n = *n.inner();
    let parts: Vec<&str> = match (usize::try_from(n), sep.is_empty()) {
        (Ok(0), _) => Vec::new(),
        (Ok(n), true) => {
            let mut parts: Vec<&str> = Vec::new();
            let mut rest: &str = this;
            while let Some(c) = rest.chars().next() {
                if parts.len() + 1 == n {
                    break;
                }
                let (head, tail) = rest.split_at(c.len_utf8());
                parts.push(head);
                rest = tail;
            }
            if !rest.is_empty() {
                parts.push(rest);
            }
            parts
        }
        (Ok(n), false) => this.splitn(n, sep.inner()).collect(),
        (Err(_), true) => this
            .char_indices()
            .map(|(i, c)| &this[i..i + c.len_utf8()])
            .collect(),
        (Err(_), false) => this.split(sep.inner()).collect(),
    };
    parts
        .into_iter()
        .map(|s| Box::new(CelString::from(s.to_owned())) as Box<dyn Val>)
        .collect::<Vec<_>>()
        .into()
}

fn substring(this: &CelString<'_>, start: &CelInt) -> Result<CelString<'static>, ExecutionError> {
    let len = this.chars().count() as i64;
    if *start.inner() < 0 || *start.inner() > len {
        return Err(out_of_range("substring", *start.inner()));
    }
    substring_range(this, start, &CelInt::from(len))
}

fn substring_range(
    this: &CelString<'_>,
    start: &CelInt,
    end: &CelInt,
) -> Result<CelString<'static>, ExecutionError> {
    let (start, end) = (*start.inner(), *end.inner());
    let len = this.chars().count() as i64;
    if start > end {
        return Err(ExecutionError::function_error(
            "substring",
            format!("invalid substring range. start: {start}, end: {end}"),
        ));
    }
    if start < 0 || start > len {
        return Err(out_of_range("substring", start));
    }
    if end > len {
        return Err(out_of_range("substring", end));
    }
    Ok(this
        .chars()
        .skip(start as usize)
        .take((end - start) as usize)
        .collect::<String>()
        .into())
}

/// `this` as a double-quoted CEL string literal: the characters CEL escapes
/// are escaped.
fn quote(this: &CelString<'_>) -> CelString<'static> {
    let mut quoted = String::with_capacity(this.len() + 2);
    quoted.push('"');
    for c in this.chars() {
        match c {
            '\x07' => quoted.push_str("\\a"),
            '\x08' => quoted.push_str("\\b"),
            '\x0c' => quoted.push_str("\\f"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\x0b' => quoted.push_str("\\v"),
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted.into()
}

/// Reverses the code points of `this`.
fn reverse(this: &CelString<'_>) -> CelString<'static> {
    this.chars().rev().collect::<String>().into()
}

#[cfg(test)]
mod tests {
    use crate::common::types::CelString;
    use crate::extensions::tests::OtherList;
    use crate::{Context, DeclarationError, Env, ExecutionError, Program, Value};
    use std::sync::Arc;

    #[test]
    fn registering_twice_is_an_error() {
        let mut env = Env::stdlib();
        assert_eq!(env.add_extension(crate::extensions::strings), Ok(()));
        assert_eq!(
            env.add_extension(crate::extensions::strings),
            Err(DeclarationError::duplicate_overload(
                "charAt",
                "string.charAt(int)"
            ))
        );
    }

    fn context() -> Context<'static, 'static> {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::strings)
            .expect("We can't test the extension, if we can't register it");
        Context::with_env(Arc::new(env))
    }

    fn eval_in(ctx: &Context, expr: &str) -> Result<Value, ExecutionError> {
        Program::compile(expr)
            .expect("This must be valid CEL")
            .execute(ctx)
    }

    fn eval(expr: &str) -> Result<Value, ExecutionError> {
        eval_in(&context(), expr)
    }

    fn assert_eval(expr: &str, expected: impl Into<Value>) {
        assert_eq!(eval(expr), Ok(expected.into()), "{expr}");
    }

    fn assert_error(expr: &str, message: &str) {
        match eval(expr) {
            Err(ExecutionError::FunctionError { message: m, .. }) => {
                assert_eq!(m, message, "{expr}")
            }
            other => panic!("{expr}: expected a function error, got {other:?}"),
        }
    }

    #[test]
    fn char_at() {
        assert_eval("'abc'.charAt(1)", "b");
        assert_eval("'tacocat'.charAt(7)", "");
        assert_eval("'©αT'.charAt(1)", "α");
        assert_error("'tacocat'.charAt(30)", "index out of range: 30");
        assert_error("'tacocat'.charAt(-1)", "index out of range: -1");
    }

    #[test]
    fn index_of() {
        assert_eval("'hello mellow'.indexOf('')", 0);
        assert_eval("'hello mellow'.indexOf('ello')", 1);
        assert_eval("'hello mellow'.indexOf('jello')", -1);
        assert_eval("'hello mellow'.indexOf('', 2)", 2);
        assert_eval("'ta©o©αT'.indexOf('©', 3)", 4);
        assert_eval("'ta©o©αT'.indexOf('©α', 5)", -1);
        assert_error(
            "'hello mellow'.indexOf('ello', 20)",
            "index out of range: 20",
        );
        assert_eval("'abc'.indexOf('', 3)", 3);
        assert_eval("'abc'.indexOf('', 20)", 3);
        assert_error("'abc'.indexOf('', -1)", "index out of range: -1");
    }

    #[test]
    fn last_index_of() {
        assert_eval("'hello mellow'.lastIndexOf('')", 12);
        assert_eval("'hello mellow'.lastIndexOf('ello')", 7);
        assert_eval("'hello mellow'.lastIndexOf('jello')", -1);
        assert_eval("''.lastIndexOf('@@')", -1);
        assert_eval("'hello mellow'.lastIndexOf('ello', 6)", 1);
        assert_eval("'ta©o©αT'.lastIndexOf('©', 3)", 2);
        assert_eval("'bananananana'.lastIndexOf('nana', 7)", 6);
        assert_error(
            "'hello mellow'.lastIndexOf('ello', 20)",
            "index out of range: 20",
        );
        assert_error("'tacocat'.lastIndexOf('a', -1)", "index out of range: -1");
        assert_eval("'abc'.lastIndexOf('', 3)", 3);
        assert_eval("'abc'.lastIndexOf('', 20)", 3);
        assert_error("'abc'.lastIndexOf('', -1)", "index out of range: -1");
    }

    #[test]
    fn join() {
        assert_eval("['hello', 'mellow'].join()", "hellomellow");
        assert_eval("[].join()", "");
        assert_eval("['hello', 'mellow'].join(' ')", "hello mellow");
        assert_eval("[].join('-')", "");
        assert!(eval("['hello', 1].join()").is_err());
    }

    #[test]
    fn join_any_list() {
        let mut ctx = context();
        ctx.add_variable_as_val(
            "names",
            Box::new(OtherList(vec![
                CelString::from("hello"),
                CelString::from("mellow"),
            ])),
        );
        assert_eq!(eval_in(&ctx, "names.join()"), Ok("hellomellow".into()));
        assert_eq!(eval_in(&ctx, "names.join(' ')"), Ok("hello mellow".into()));
    }

    #[test]
    fn ascii_casing() {
        assert_eval("'TacoCat'.lowerAscii()", "tacocat");
        assert_eval("'TacoCÆt Xii'.lowerAscii()", "tacocÆt xii");
        assert_eval("'TacoCat'.upperAscii()", "TACOCAT");
        assert_eval("'TacoCÆt Xii'.upperAscii()", "TACOCÆT XII");
    }

    #[test]
    fn trim() {
        assert_eval("'  \ttrim\t    '.trim()", "trim");
        assert_eval("'\\u0085\\u00a0\\u1680text'.trim()", "text");
        assert_eval("'\\u180etext\\u200b'.trim()", "\u{180e}text\u{200b}");
    }

    #[test]
    fn replace() {
        assert_eval("'hello hello'.replace('he', 'we')", "wello wello");
        assert_eval("'hello hello'.replace('he', 'we', -1)", "wello wello");
        assert_eval("'hello hello'.replace('he', 'we', 1)", "wello hello");
        assert_eval("'hello hello'.replace('he', 'we', 0)", "hello hello");
        assert_eval("'hello hello'.replace('', '_')", "_h_e_l_l_o_ _h_e_l_l_o_");
        assert_eval("'hello hello'.replace('h', '')", "ello ello");
    }

    #[test]
    fn split() {
        assert_eval(
            "'hello hello hello'.split(' ')",
            vec!["hello", "hello", "hello"],
        );
        assert_eval(
            "'hello hello hello'.split(' ', 0)",
            Value::List(vec![].into()),
        );
        assert_eval(
            "'hello hello hello'.split(' ', 1)",
            vec!["hello hello hello"],
        );
        assert_eval(
            "'hello hello hello'.split(' ', 2)",
            vec!["hello", "hello hello"],
        );
        assert_eval(
            "'hello hello hello'.split(' ', -1)",
            vec!["hello", "hello", "hello"],
        );
        assert_eval("'o©o©o©o'.split('©', -1)", vec!["o", "o", "o", "o"]);
        assert_eval("'a©c'.split('')", vec!["a", "©", "c"]);
        assert_eval("'a©c'.split('', 2)", vec!["a", "©c"]);
    }

    #[test]
    fn substring() {
        assert_eval("'tacocat'.substring(4)", "cat");
        assert_eval("'tacocat'.substring(7)", "");
        assert_eval("'tacocat'.substring(0, 4)", "taco");
        assert_eval("'ta©o©αT'.substring(2, 6)", "©o©α");
        assert_error("'tacocat'.substring(40)", "index out of range: 40");
        assert_error("'tacocat'.substring(-1)", "index out of range: -1");
        assert_error("'tacocat'.substring(1, 50)", "index out of range: 50");
        assert_error("'tacocat'.substring(49, 50)", "index out of range: 49");
        assert_error(
            "'tacocat'.substring(4, 3)",
            "invalid substring range. start: 4, end: 3",
        );
    }

    #[test]
    fn quote() {
        assert_eval("strings.quote('verbatim')", "\"verbatim\"");
        assert_eval("strings.quote('')", "\"\"");
        assert_eval(
            r#"strings.quote("\a\b\f\n\r\t\v\\\"")"#,
            r#""\a\b\f\n\r\t\v\\\"""#,
        );
        assert_eval("strings.quote('ta©o©αT')", "\"ta©o©αT\"");
    }

    #[test]
    fn reverse() {
        assert_eval("''.reverse()", "");
        assert_eval("'gums'.reverse()", "smug");
        assert_eval("'Ta©oCαt'.reverse()", "tαCo©aT");
    }

    #[test]
    fn format_substitutes_in_order() {
        assert_eval("'no substitution'.format([])", "no substitution");
        assert_eval("'%% and %%'.format([])", "% and %");
        assert_eval("'%%%s%%'.format(['text'])", "%text%");
        assert_eval("'%s, %d'.format(['a', 1, 'ignored'])", "a, 1");
        assert_eval("'©%sα'.format(['T'])", "©Tα");
    }

    #[test]
    fn format_strings() {
        assert_eval("'%s'.format([null])", "null");
        assert_eval("'%s %s'.format([true, false])", "true false");
        assert_eval("'%s %s %s'.format([-1, 2u, 2.5])", "-1 2 2.5");
        assert_eval("'%s'.format([1e21])", "1000000000000000000000");
        assert_eval("'%s'.format([b'xyz'])", "xyz");
        assert_eval("'%s'.format([b'\\xff\\xfe!'])", "\u{fffd}!");
        assert_eval("'%s'.format([type('')])", "string");
        assert_eval(
            "'%s'.format([timestamp('2023-02-03T23:31:20.5+01:00')])",
            "2023-02-03T22:31:20.5Z",
        );
        assert_eval("'%s'.format([duration('1h45m47.25s')])", "6347.25s");
        assert_eval("'%s'.format([duration('-1.5s')])", "-1.5s");
        assert_eval(
            "'%s'.format([[1, 'a', [double('NaN')], {}]])",
            "[1, a, [NaN], {}]",
        );
        assert_eval(
            "'%s'.format([{'b': 2, 1: [], true: -1.0}])",
            "{1: [], b: 2, true: -1}",
        );
    }

    #[test]
    fn format_numbers() {
        assert_eval("'%d %d %d'.format([-12, 12u, 1.5])", "-12 12 1.5");
        assert_eval(
            "'%f %.0f %.2f'.format([2, 2.5, -1.005])",
            "2.000000 2 -1.00",
        );
        assert_eval("'%.1f'.format([double('-Infinity')])", "-Infinity");
        assert_eval("'%e'.format([1052.032911275])", "1.052033e+03");
        assert_eval("'%.1e %.0e'.format([-0.000314, 3u])", "-3.1e-04 3e+00");
        assert_eval("'%e'.format([1e100])", "1.000000e+100");
        assert_eval("'%e'.format([double('NaN')])", "NaN");
        assert_eval("'%b %b %b'.format([-5, 5u, true])", "-101 101 1");
        assert_eval("'%o %o'.format([-8, 8u])", "-10 10");
        assert_eval("'%x %X'.format([-255, 255u])", "-ff FF");
        assert_eval("'%x %X'.format(['Hi!', b'\\x0a\\xff'])", "486921 0AFF");
    }

    #[test]
    fn format_errors() {
        assert_error("'%d %d'.format([1])", "index 1 out of range");
        assert_error("'%'.format([])", "index 0 out of range");
        assert_error("'%'.format([1])", "unexpected end of string");
        assert_error(
            "'%a'.format([1])",
            "could not parse formatting clause: unrecognized formatting clause \"a\"",
        );
        assert_error(
            "'%.f'.format([1])",
            "could not parse formatting clause: error while parsing precision: \
             error while converting precision to integer: cannot parse integer from empty string",
        );
        assert_error(
            "'%d'.format(['1'])",
            "error during formatting: decimal clause can only be used on ints, uints, and doubles, \
             was given string",
        );
        assert_error(
            "'%f'.format([null])",
            "error during formatting: fixed-point clause can only be used on ints, uints, and \
             doubles, was given null_type",
        );
        assert_error(
            "'%e'.format([true])",
            "error during formatting: scientific clause can only be used on ints, uints, and \
             doubles, was given bool",
        );
        assert_error(
            "'%b'.format([1.0])",
            "error during formatting: only ints, uints, and bools can be formatted as binary, \
             was given double",
        );
        assert_error(
            "'%x'.format([[]])",
            "error during formatting: only ints, uints, bytes, and strings can be formatted as \
             hex, was given list",
        );
        assert_error(
            "'%o'.format([duration('1s')])",
            "error during formatting: octal clause can only be used on ints and uints, was \
             given google.protobuf.Duration",
        );
        assert_error(
            "'%s'.format([[optional.none()]])",
            "error during formatting: string clause can only be used on strings, bools, bytes, \
             ints, doubles, maps, lists, types, durations, and timestamps, was given \
             optional_type",
        );
    }

    #[test]
    fn format_any_list() {
        let mut ctx = context();
        ctx.add_variable_as_val(
            "names",
            Box::new(OtherList(vec![
                CelString::from("hello"),
                CelString::from("mellow"),
            ])),
        );
        assert_eq!(
            eval_in(&ctx, "'%s %s'.format(names)"),
            Ok("hello mellow".into())
        );
        assert_eq!(
            eval_in(&ctx, "'%s'.format([names])"),
            Ok("[hello, mellow]".into())
        );
    }
}
