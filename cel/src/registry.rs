use crate::common::types::Type;
#[cfg(feature = "structs")]
use crate::common::value::{CowVal, Val};
use crate::DeclarationError;
#[cfg(feature = "structs")]
use crate::ExecutionError;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// A struct type that expressions can construct, e.g. `acme.Account{id: 1}`.
///
/// [`StructDef`](crate::StructDef) is one; implementing this trait lets
/// a struct literal construct any other [`Val`], e.g. a Rust type of your own.
#[cfg(feature = "structs")]
pub trait StructType: Send + Sync {
    /// The struct's type: its name is the one struct literals construct it
    /// by, and the one it is registered under.
    fn get_type(&self) -> &Type;

    /// Creates a value of this type from the fields of a struct literal.
    ///
    /// # Errors
    ///
    /// Fails when the fields do not make a value of this type, e.g. an
    /// unknown field or a value of the wrong type.
    fn new_value<'b, 'v>(
        &self,
        fields: BTreeMap<String, CowVal<'b, 'v>>,
    ) -> Result<Box<dyn Val + 'v>, ExecutionError>;
}

/// The types known to an [`Env`](crate::Env), by name.
///
/// A registered type's name can be used in an expression, where it resolves
/// to the type value, e.g. `type(1) == int`. The registry starts out empty:
/// the environment's libraries register their types, e.g. the standard
/// library registers `int` and `optional_type`.
///
/// A struct type registered with [`StructType`] can also be constructed.
#[derive(Default)]
pub struct TypeRegistry {
    types: BTreeMap<String, Type>,
    #[cfg(feature = "structs")]
    structs: BTreeMap<String, Box<dyn StructType>>,
}

impl Debug for TypeRegistry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("TypeRegistry");
        debug.field("types", &self.types);
        #[cfg(feature = "structs")]
        debug.field("structs", &self.structs.keys());
        debug.finish()
    }
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

    /// Finds the struct type registered under `name`.
    #[cfg(feature = "structs")]
    pub fn find_struct(&self, name: &str) -> Option<&dyn StructType> {
        self.structs.get(name).map(Box::as_ref)
    }

    /// Registers `s` and its type under its type's name.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::TypeConflict`] if a struct type, or
    /// a type other than `s`'s, is registered under the same name, and with
    /// [`DeclarationError::InvalidTypeName`] as [`register`](Self::register)
    /// does. A failed registration leaves the registry as it was.
    #[cfg(feature = "structs")]
    pub(crate) fn register_struct(
        &mut self,
        s: Box<dyn StructType>,
    ) -> Result<(), DeclarationError> {
        let name = s.get_type().name();
        if self.structs.contains_key(name) {
            return Err(DeclarationError::type_conflict(name));
        }
        self.register(s.get_type().to_owned())?;
        self.structs.insert(name.to_owned(), s);
        Ok(())
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

    #[cfg(feature = "structs")]
    mod structs {
        use super::super::*;
        use crate::common::types::{Kind, INT_TYPE};
        use crate::StructDef;

        #[test]
        fn a_struct_is_registered_with_its_type() {
            let mut registry = TypeRegistry::default();
            assert_eq!(
                registry.register_struct(Box::new(StructDef::new("acme.Account".into()))),
                Ok(())
            );
            let s = registry.find_struct("acme.Account").unwrap();
            assert_eq!(s.get_type().name(), "acme.Account");
            let t = registry.find_type("acme.Account").unwrap();
            assert_eq!(t.kind(), Kind::Struct);
        }

        #[test]
        fn a_struct_of_a_registered_name_is_a_conflict() {
            let mut registry = TypeRegistry::default();
            registry
                .register_struct(Box::new(StructDef::new("acme.Account".into())))
                .unwrap();
            assert_eq!(
                registry.register_struct(Box::new(
                    StructDef::new("acme.Account".into()).add_field("id".into(), INT_TYPE)
                )),
                Err(DeclarationError::type_conflict("acme.Account"))
            );

            registry.register(INT_TYPE).unwrap();
            assert_eq!(
                registry.register_struct(Box::new(StructDef::new("int".into()))),
                Err(DeclarationError::type_conflict("int"))
            );
            assert!(registry.find_struct("int").is_none());
            assert_eq!(registry.find_type("int"), Some(&INT_TYPE));
        }

        /// Its type may be registered before the struct is.
        #[test]
        fn a_struct_of_a_registered_equal_type_is_registered() {
            let mut registry = TypeRegistry::default();
            registry
                .register(Type::new_struct_type("acme.Account"))
                .unwrap();
            assert_eq!(
                registry.register_struct(Box::new(StructDef::new("acme.Account".into()))),
                Ok(())
            );
            assert!(registry.find_struct("acme.Account").is_some());
        }

        #[test]
        fn a_struct_of_an_invalid_name_is_not_registered() {
            let mut registry = TypeRegistry::default();
            assert_eq!(
                registry.register_struct(Box::new(StructDef::new("acme-Account".into()))),
                Err(DeclarationError::invalid_type_name("acme-Account"))
            );
            assert!(registry.find_struct("acme-Account").is_none());
        }
    }
}
