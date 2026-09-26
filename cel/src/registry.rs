use crate::common::types::Type;
use crate::DeclarationError;
use std::collections::BTreeMap;

/// The types known to an [`Env`](crate::Env), by name.
///
/// A registered type's name can be used in an expression, where it resolves
/// to the type value, e.g. `type(1) == int`. The registry starts out empty:
/// the environment's libraries register their types, e.g. the standard
/// library registers `int` and `optional_type`.
#[derive(Debug, Default)]
pub struct TypeRegistry {
    types: BTreeMap<String, Type>,
}

impl TypeRegistry {
    /// Finds the type registered under `name`.
    pub fn find_type(&self, name: &str) -> Option<&Type> {
        self.types.get(name)
    }

    /// Registers `t` under its name.
    ///
    /// Registering a type that is already registered is a no-op, so that
    /// libraries can each register a type they share.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::TypeConflict`] if another type is
    /// registered under the same name, and with
    /// [`DeclarationError::InvalidTypeName`] if the name is not a, possibly
    /// qualified, identifier, e.g. `int` or `my.pkg.Type`, which no
    /// expression could refer to.
    pub(crate) fn register(&mut self, t: Type) -> Result<(), DeclarationError> {
        if !is_qualified_ident(t.name()) {
            return Err(DeclarationError::invalid_type_name(t.name()));
        }
        match self.types.get(t.name()) {
            Some(existing) if *existing == t => Ok(()),
            Some(_) => Err(DeclarationError::type_conflict(t.name())),
            None => {
                self.types.insert(t.name().to_owned(), t);
                Ok(())
            }
        }
    }
}

/// Whether `name` is an identifier, or several separated by dots.
fn is_qualified_ident(name: &str) -> bool {
    name.split('.').all(|ident| {
        let mut chars = ident.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::{Kind, INT_TYPE, OPTIONAL_TYPE};

    #[test]
    fn a_new_registry_is_empty() {
        let registry = TypeRegistry::default();
        assert!(registry.find_type("int").is_none());
        assert!(registry.find_type("optional_type").is_none());
    }

    #[test]
    fn a_type_is_found_by_its_name() {
        let mut registry = TypeRegistry::default();
        assert_eq!(
            registry.register(Type::new_opaque_type("my.pkg.Ip")),
            Ok(())
        );
        let t = registry.find_type("my.pkg.Ip").unwrap();
        assert_eq!(t.kind(), Kind::Opaque);
        assert_eq!(t.name(), "my.pkg.Ip");
    }

    #[test]
    fn registering_the_same_type_again_is_a_no_op() {
        let mut registry = TypeRegistry::default();
        assert_eq!(registry.register(OPTIONAL_TYPE), Ok(()));
        assert_eq!(registry.register(OPTIONAL_TYPE), Ok(()));
        assert_eq!(registry.find_type("optional_type"), Some(&OPTIONAL_TYPE));
    }

    #[test]
    fn another_type_of_the_same_name_is_a_conflict() {
        let mut registry = TypeRegistry::default();
        registry.register(INT_TYPE).unwrap();
        assert_eq!(
            registry.register(Type::new_opaque_type("int")),
            Err(DeclarationError::type_conflict("int"))
        );
        assert_eq!(registry.find_type("int"), Some(&INT_TYPE));

        registry.register(Type::new_opaque_type("Ip")).unwrap();
        assert_eq!(
            registry.register(Type::simple_type(Kind::Opaque, "Ip")),
            Ok(()),
            "an equal type, however built"
        );
        assert_eq!(
            registry.register(Type::simple_type(Kind::Unspecified, "Ip")),
            Err(DeclarationError::type_conflict("Ip"))
        );
    }

    #[test]
    fn a_name_no_expression_can_refer_to_is_invalid() {
        let mut registry = TypeRegistry::default();
        for name in ["", ".", "a.", ".a", "a..b", "1a", "a-b", "a b", "`a`"] {
            assert_eq!(
                registry.register(Type::new_opaque_type(name)),
                Err(DeclarationError::invalid_type_name(name)),
                "{name:?}"
            );
            assert!(registry.find_type(name).is_none(), "{name:?}");
        }
        for name in ["a", "_a", "a1", "a_b.C", "my.pkg.Type"] {
            assert_eq!(
                registry.register(Type::new_opaque_type(name)),
                Ok(()),
                "{name:?}"
            );
        }
    }
}
