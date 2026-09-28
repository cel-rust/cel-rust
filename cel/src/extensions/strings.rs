//! The strings extension library: member functions on `string` (and
//! `list(string).join`), following cel-go's `ext.Strings()`.
//!
//! Indices are in code points, not bytes, as in the rest of CEL.

use crate::common::types::{CelInt, CelList, CelString};
use crate::common::value::Val;
use crate::{Env, ExecutionError};

/// Registers the strings extension's overloads on `env`.
pub fn extension(env: &mut Env) {
    crate::add_member_overload!(env, fn char_at: (CelString, CelInt) -> Result<CelString>);
    crate::add_member_overload!(env, fn index_of: (CelString, CelString) -> CelInt);
    crate::add_member_overload!(env, fn index_of_offset: (CelString, CelString, CelInt) -> Result<CelInt>,
        name = "indexOf");
    crate::add_member_overload!(env, fn last_index_of: (CelString, CelString) -> CelInt);
    crate::add_member_overload!(env, fn last_index_of_offset: (CelString, CelString, CelInt) -> Result<CelInt>,
        name = "lastIndexOf");
    crate::add_member_overload!(env, fn join: (CelList) -> Result<CelString>);
    crate::add_member_overload!(env, fn join_sep: (CelList, CelString) -> Result<CelString>,
        name = "join");
    crate::add_member_overload!(env, fn lower_ascii: (CelString) -> CelString);
    crate::add_member_overload!(env, fn upper_ascii: (CelString) -> CelString);
    crate::add_member_overload!(env, fn trim: (CelString) -> CelString);
    crate::add_member_overload!(env, fn replace: (CelString, CelString, CelString) -> CelString);
    crate::add_member_overload!(env, fn replace_n: (CelString, CelString, CelString, CelInt) -> CelString,
        name = "replace");
    crate::add_member_overload!(env, fn split: (CelString, CelString) -> CelList);
    crate::add_member_overload!(env, fn split_n: (CelString, CelString, CelInt) -> CelList,
        name = "split");
    crate::add_member_overload!(env, fn substring: (CelString, CelInt) -> Result<CelString>);
    crate::add_member_overload!(env, fn substring_range: (CelString, CelInt, CelInt) -> Result<CelString>,
        name = "substring");
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

fn join(this: &CelList<'_>) -> Result<CelString<'static>, ExecutionError> {
    join_with(this, "")
}

fn join_sep(this: &CelList<'_>, sep: &CelString<'_>) -> Result<CelString<'static>, ExecutionError> {
    join_with(this, sep.inner())
}

fn join_with(list: &CelList<'_>, sep: &str) -> Result<CelString<'static>, ExecutionError> {
    let parts = list
        .iter()
        .map(|v| {
            v.downcast_ref::<CelString>()
                .map(CelString::inner)
                .ok_or_else(|| ExecutionError::UnexpectedType {
                    got: v.get_type().name().to_owned(),
                    want: CelString::cel_type().name().to_owned(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(parts.join(sep).into())
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

#[cfg(test)]
mod tests {
    use crate::{Context, Env, ExecutionError, Program, Value};
    use std::sync::Arc;

    fn eval(expr: &str) -> Result<Value, ExecutionError> {
        let mut env = Env::stdlib();
        env.add_extension(crate::extensions::strings);
        let ctx = Context::with_env(Arc::new(env));
        Program::compile(expr)
            .expect("This must be valid CEL")
            .execute(&ctx)
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
}
