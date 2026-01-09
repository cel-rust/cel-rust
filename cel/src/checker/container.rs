//! Container resolution for qualified identifiers.
//!
//! This module implements C++ namespace-style resolution for CEL qualified identifiers.
//! When a container is specified (e.g., "cel.expr.conformance.proto2"), qualified names
//! are searched with progressively shorter prefixes.

/// Resolves candidate names for a qualified identifier using C++ namespace rules.
///
/// Given a container and a name, returns candidate names in resolution order
/// (most qualified first, unqualified last).
///
/// # Examples
///
/// ```ignore
/// // With container "a.b.c" and name "R.s":
/// // Returns ["a.b.c.R.s", "a.b.R.s", "a.R.s", "R.s"]
/// let candidates = resolve_candidate_names(Some("a.b.c"), "R.s");
///
/// // With no container:
/// // Returns ["R.s"]
/// let candidates = resolve_candidate_names(None, "R.s");
///
/// // With leading dot (absolute reference):
/// // Returns [".R.s"] (only the absolute name)
/// let candidates = resolve_candidate_names(Some("a.b.c"), ".R.s");
/// ```
pub fn resolve_candidate_names(container: Option<&str>, name: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    // Handle absolute references (leading dot)
    if name.starts_with('.') {
        // Absolute reference - only try the exact name (without the leading dot)
        candidates.push(name[1..].to_string());
        return candidates;
    }

    // Generate candidates with progressively shorter container prefixes
    if let Some(container) = container {
        if !container.is_empty() {
            let mut parts: Vec<&str> = container.split('.').collect();
            while !parts.is_empty() {
                candidates.push(format!("{}.{}", parts.join("."), name));
                parts.pop();
            }
        }
    }

    // Always include the unqualified name last
    candidates.push(name.to_string());
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_container() {
        let candidates = resolve_candidate_names(None, "Foo");
        assert_eq!(candidates, vec!["Foo"]);
    }

    #[test]
    fn test_empty_container() {
        let candidates = resolve_candidate_names(Some(""), "Foo");
        assert_eq!(candidates, vec!["Foo"]);
    }

    #[test]
    fn test_simple_container() {
        let candidates = resolve_candidate_names(Some("a"), "Foo");
        assert_eq!(candidates, vec!["a.Foo", "Foo"]);
    }

    #[test]
    fn test_nested_container() {
        let candidates = resolve_candidate_names(Some("a.b.c"), "R.s");
        assert_eq!(candidates, vec!["a.b.c.R.s", "a.b.R.s", "a.R.s", "R.s"]);
    }

    #[test]
    fn test_proto_container() {
        let candidates =
            resolve_candidate_names(Some("cel.expr.conformance.proto2"), "TestAllTypes");
        assert_eq!(
            candidates,
            vec![
                "cel.expr.conformance.proto2.TestAllTypes",
                "cel.expr.conformance.TestAllTypes",
                "cel.expr.TestAllTypes",
                "cel.TestAllTypes",
                "TestAllTypes"
            ]
        );
    }

    #[test]
    fn test_absolute_reference() {
        let candidates = resolve_candidate_names(Some("a.b.c"), ".Absolute.Name");
        assert_eq!(candidates, vec!["Absolute.Name"]);
    }

    #[test]
    fn test_qualified_name() {
        // When the name itself contains dots
        let candidates = resolve_candidate_names(Some("pkg"), "sub.Type");
        assert_eq!(candidates, vec!["pkg.sub.Type", "sub.Type"]);
    }
}
