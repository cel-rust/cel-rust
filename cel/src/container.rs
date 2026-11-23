use crate::registry::is_qualified_ident;
use crate::DeclarationError;
use std::borrow::Cow;
use std::iter::{once, successors};

/// The namespace an [`Env`](crate::Env) resolves names against, letting an
/// expression written for container `x` say `y` where it would otherwise
/// need `x.y`. Mirrors cel-go's `containers.Container`, without aliases.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Container {
    name: String,
}

impl Container {
    /// A container named `name`, an identifier, possibly qualified, such as
    /// `a.b.c`, or the empty name for no container.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::InvalidContainerName`] for any other
    /// name, a leading dot included.
    pub(crate) fn new(name: &str) -> Result<Self, DeclarationError> {
        if !name.is_empty() && !is_qualified_ident(name) {
            return Err(DeclarationError::invalid_container_name(name));
        }
        Ok(Container {
            name: name.to_owned(),
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The names `name` may refer to, most specific first, ending with `name`
    /// itself: in container `a.b.c`, `R.s` is `a.b.c.R.s`, `a.b.R.s`,
    /// `a.R.s`, then `R.s`. A leading dot makes `name` absolute, so it is the
    /// only candidate, without the dot, as `name` is without a container.
    /// Mirrors cel-go's `Container.ResolveCandidateNames`.
    pub(crate) fn candidates<'a, 'n: 'a>(&'a self, name: &'n str) -> Candidates<'a, 'n> {
        match name.strip_prefix('.') {
            Some(absolute) => Candidates::One(absolute),
            None if self.name.is_empty() => Candidates::One(name),
            None => Candidates::Many {
                container: &self.name,
                name,
            },
        }
    }
}

/// The names a name may refer to under a [`Container`]: see
/// [`Container::candidates`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum Candidates<'a, 'n> {
    /// The name alone, as without a container or for an absolute name.
    One(&'n str),
    /// `name` under `container`, then under each of its parents, then alone.
    Many { container: &'a str, name: &'n str },
}

impl<'a, 'n: 'a> Candidates<'a, 'n> {
    /// The first `Some` `f` gives for a candidate, most specific first. The
    /// only candidate, as most names have, is neither listed nor allocated.
    pub(crate) fn find_map<T>(self, mut f: impl FnMut(&str) -> Option<T>) -> Option<T> {
        match self {
            Candidates::One(name) => f(name),
            Candidates::Many { .. } => self.iter().find_map(|candidate| f(&candidate)),
        }
    }

    /// Each candidate, most specific first.
    pub(crate) fn iter(self) -> impl Iterator<Item = Cow<'n, str>> + 'a {
        let (scope, name) = match self {
            Candidates::One(name) => ("", name),
            Candidates::Many { container, name } => (container, name),
        };
        successors(Some(scope).filter(|s| !s.is_empty()), |s| {
            s.rfind('.').map(|i| &s[..i])
        })
        .map(move |scope| Cow::Owned(format!("{scope}.{name}")))
        .chain(once(Cow::Borrowed(name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates(container: &str, name: &str) -> Vec<String> {
        let container = Container::new(container).unwrap();
        container
            .candidates(name)
            .iter()
            .map(Cow::into_owned)
            .collect()
    }

    #[test]
    fn candidates_go_from_the_most_specific_container_to_the_bare_name() {
        assert_eq!(
            candidates("a.b.c", "R.s"),
            ["a.b.c.R.s", "a.b.R.s", "a.R.s", "R.s"]
        );
    }

    #[test]
    fn without_a_container_the_name_is_the_only_candidate() {
        assert_eq!(candidates("", "y"), ["y"]);
    }

    #[test]
    fn a_leading_dot_makes_the_name_absolute() {
        assert_eq!(candidates("a.b", ".R.s"), ["R.s"]);
        assert_eq!(candidates("", ".y"), ["y"]);
    }

    #[test]
    fn find_map_visits_the_candidates_iter_lists() {
        for (container, name) in [("", "y"), ("", ".y"), ("a.b", "R.s"), ("a.b", ".R.s")] {
            let mut visited = Vec::new();
            let found = Container::new(container)
                .unwrap()
                .candidates(name)
                .find_map(|candidate| {
                    visited.push(candidate.to_owned());
                    None::<()>
                });
            assert_eq!(found, None, "{container:?} {name:?}");
            assert_eq!(
                visited,
                candidates(container, name),
                "{container:?} {name:?}"
            );
        }
    }

    #[test]
    fn find_map_stops_at_the_first_candidate_that_gives_a_value() {
        let mut visited = Vec::new();
        let found = Container::new("a.b.c")
            .unwrap()
            .candidates("R.s")
            .find_map(|candidate| {
                visited.push(candidate.to_owned());
                (candidate == "a.b.R.s").then_some(candidate.len())
            });
        assert_eq!(found, Some("a.b.R.s".len()));
        assert_eq!(visited, ["a.b.c.R.s", "a.b.R.s"]);
    }

    #[test]
    fn a_container_must_be_a_qualified_identifier() {
        assert!(Container::new("").is_ok());
        assert!(Container::new("a.b").is_ok());
        for name in [".a", "a.", "a..b", "a-b"] {
            assert_eq!(
                Container::new(name),
                Err(DeclarationError::invalid_container_name(name))
            );
        }
    }
}
