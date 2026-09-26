use crate::common::{
    decls::FunctionDecl,
    functions::Function,
    types::{self, Type},
    value::CowVal,
};
use crate::registry::TypeRegistry;
use crate::DeclarationError;
#[cfg(feature = "structs")]
use crate::{common::types::CelStruct, common::value::Val, ExecutionError, StructType};
use std::collections::{
    btree_map::Entry::{Occupied, Vacant},
    BTreeMap,
};

/// An environment for the CEL execution.
///
/// This is where functions, overloads, and custom structs are defined.
///
/// # Example
///
/// ## Custom Structs
///
/// You can define custom struct types that can be instantiated from CEL expressions.
///
/// ```
/// #[cfg(feature = "structs")]
/// {
/// use cel::{Env, StructDef, common::types, common::types::CelString};
///
/// let mut env = Env::stdlib();
/// env.add_struct(
///     StructDef::new("cel.MyStruct".to_owned())
///         .add_field("some_field".to_owned(), types::STRING_TYPE)
///         .add_field_with_default("with_default".to_owned(), Box::new(CelString::from("default_value")))
/// ).unwrap();
/// }
/// ```
///
/// ## Function Overloads
///
/// You can add custom function overloads to the environment.
///
/// ```
/// use cel::{Env, common::types, common::value::CowVal};
///
/// let mut env = Env::stdlib();
///
/// // Define a function that takes an integer and returns its square.
/// env.add_overload("square", "int_square", vec![types::INT_TYPE], |args| {
///     let val = args[0].downcast_ref::<types::CelInt>().unwrap();
///     Ok(CowVal::owned(types::CelInt::from(val.inner() * val.inner())))
/// }).unwrap();
/// ```
pub struct Env {
    functions: BTreeMap<String, FunctionDecl>,
    types: TypeRegistry,
    error_on_duplicate_map_keys: bool,
}

impl Default for Env {
    fn default() -> Self {
        Env {
            functions: BTreeMap::new(),
            types: TypeRegistry::default(),
            error_on_duplicate_map_keys: true,
        }
    }
}

impl Env {
    /// Returns the standard library environment.
    ///
    /// This environment contains all the standard functions and types as defined by the
    /// CEL specification.
    pub fn stdlib() -> Env {
        let mut env = Env::default();
        types::bool::stdlib(&mut env);
        types::bytes::stdlib(&mut env);
        types::double::stdlib(&mut env);
        types::r#dyn::stdlib(&mut env);
        types::int::stdlib(&mut env);
        types::list::stdlib(&mut env);
        types::map::stdlib(&mut env);
        types::null::stdlib(&mut env);
        types::optional::stdlib(&mut env);
        types::string::stdlib(&mut env);
        types::type_val::stdlib(&mut env);
        types::uint::stdlib(&mut env);

        #[cfg(feature = "chrono")]
        {
            types::duration::stdlib(&mut env);
            types::timestamp::stdlib(&mut env);
        }
        env
    }

    /// Adds a global function overload to the environment.
    ///
    /// The name is the function name (e.g., `_==_`, `size`).
    /// The id is the unique identifier for this overload (e.g., `equals_int64`).
    /// The args are the expected argument types.
    /// The op is the function implementation.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::DuplicateOverload`] if an overload of that
    /// name is already declared with the same id, or with the same signature.
    pub fn add_overload(
        &mut self,
        name: &str,
        id: &str,
        args: Vec<types::Type>,
        op: Function,
    ) -> Result<(), DeclarationError> {
        match self.functions.entry(name.to_owned()) {
            Vacant(vacant_entry) => {
                let mut value = FunctionDecl::new(name);
                value.add_overload(id.to_string(), false, args, op)?;
                vacant_entry.insert(value);
                Ok(())
            }
            Occupied(occupied_entry) => {
                occupied_entry
                    .into_mut()
                    .add_overload(id.to_string(), false, args, op)
            }
        }
    }

    /// Finds a global function overload that matches the given name and arguments.
    pub fn find_overload(&self, name: &str, args: &[CowVal<'_, '_>]) -> Option<Function> {
        match self.functions.get(name) {
            None => None,
            Some(fn_decl) => fn_decl.find_overload(false, args),
        }
    }

    pub(crate) fn has_overload(&self, name: &str) -> bool {
        self.functions
            .get(name)
            .is_some_and(|function| function.has_overload(false))
    }

    /// Adds a member function overload to the environment.
    ///
    /// A member function is one that is called using the receiver syntax (e.g., `x.matches(y)`).
    /// The name is the function name.
    /// The id is the unique identifier for this overload.
    /// The target is the type of the receiver.
    /// The args are the expected argument types (excluding the receiver).
    /// The op is the function implementation.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::DuplicateOverload`] if an overload of that
    /// name is already declared with the same id, or with the same signature.
    pub fn add_member_overload(
        &mut self,
        name: &str,
        id: &str,
        target: Type,
        args: Vec<types::Type>,
        op: Function,
    ) -> Result<(), DeclarationError> {
        let mut args = args;
        args.insert(0, target);
        match self.functions.entry(name.to_owned()) {
            Vacant(vacant_entry) => {
                let mut value = FunctionDecl::new(name);
                value.add_overload(id.to_string(), true, args, op)?;
                vacant_entry.insert(value);
                Ok(())
            }
            Occupied(occupied_entry) => {
                occupied_entry
                    .into_mut()
                    .add_overload(id.to_string(), true, args, op)
            }
        }
    }

    /// Finds a member function overload that matches the given name and arguments.
    pub(crate) fn find_member_overload(
        &self,
        name: &str,
        args: &[CowVal<'_, '_>],
    ) -> Option<Function> {
        match self.functions.get(name) {
            None => None,
            Some(fn_decl) => fn_decl.find_overload(true, args),
        }
    }

    pub(crate) fn has_member_overload(&self, name: &str) -> bool {
        self.functions
            .get(name)
            .is_some_and(|function| function.has_overload(true))
    }

    /// Registers a type with the environment, so that expressions can refer
    /// to it by name.
    ///
    /// The name resolves to the type value, unless a variable of the same name
    /// shadows it. Values need not be registered to be evaluated: registering
    /// their type is what lets an expression name it.
    ///
    /// ```
    /// use cel::{Context, Env, Program, Value};
    /// use cel::common::types::Type;
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.add_type(Type::new_opaque_type("Ip")).unwrap();
    /// let context = Context::with_env(Arc::new(env));
    ///
    /// let program = Program::compile("type(Ip) == type").unwrap();
    /// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::TypeConflict`] if another type is
    /// already registered under that name, and with
    /// [`DeclarationError::InvalidTypeName`] if the name is not an identifier,
    /// or several separated by dots. Registering an equal type again is fine.
    pub fn add_type(&mut self, t: Type) -> Result<(), DeclarationError> {
        self.types.register(t)
    }

    /// The types registered with the environment.
    pub fn types(&self) -> &TypeRegistry {
        &self.types
    }

    /// Adds a struct type to the environment, so that struct literals can
    /// construct it, e.g. `cel.MyStruct{some_field: 'value'}`.
    ///
    /// Its type is registered too, as [`add_type`](Self::add_type) does, so
    /// that expressions can name it: `type(x) == cel.MyStruct`.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::TypeConflict`] if a struct type, or
    /// another type, is already registered under that name, and with
    /// [`DeclarationError::InvalidTypeName`] if the name is not an identifier,
    /// or several separated by dots.
    #[cfg(feature = "structs")]
    pub fn add_struct(&mut self, s: impl StructType + 'static) -> Result<(), DeclarationError> {
        self.types.register_struct(Box::new(s))
    }

    /// Sets whether a map literal that repeats a key is an error.
    ///
    /// On by default, as the spec requires. Turning it off keeps the last entry
    /// instead, so `{'a': 1, 'a': 2}` evaluates to `{'a': 2}`, which is what
    /// cel-go does. Mirrors cel-java's `CelOptions.errorOnDuplicateMapKeys`,
    /// where the shipped default (`CelOptions.DEFAULT`) also errors.
    ///
    /// ```
    /// use cel::{Context, Env, Program, Value};
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.set_error_on_duplicate_map_keys(false);
    /// let context = Context::with_env(Arc::new(env));
    ///
    /// let program = Program::compile("{'a': 1, 'a': 2}['a']").unwrap();
    /// let value: Value = program.execute(&context).unwrap();
    /// assert_eq!(value, 2.into());
    /// ```
    pub fn set_error_on_duplicate_map_keys(&mut self, value: bool) {
        self.error_on_duplicate_map_keys = value;
    }

    pub(crate) fn error_on_duplicate_map_keys(&self) -> bool {
        self.error_on_duplicate_map_keys
    }
}

/// A definition for a custom struct type.
///
/// A struct definition defines the name of the struct, its fields, and any default values
/// for those fields. Struct definitions are added to an [`Env`] to allow them to be
/// instantiated from CEL expressions.
///
/// # Example
///
/// ```
/// use cel::{Env, StructDef, common::types, common::types::CelString};
///
/// let mut env = Env::stdlib();
/// env.add_struct(
///     StructDef::new("MyStruct".to_owned())
///         .add_field("some_field".to_owned(), types::STRING_TYPE)
///         .add_field_with_default("with_default".to_owned(), Box::new(CelString::from("default_value")))
/// ).unwrap();
/// ```
#[cfg(feature = "structs")]
pub struct StructDef {
    r#type: Type,
    fields: BTreeMap<String, Type>,
    defaults: BTreeMap<String, Box<dyn Val>>,
}

#[cfg(feature = "structs")]
impl StructDef {
    /// Creates a new struct definition with the given name.
    ///
    /// The name should be the fully qualified name of the struct as it will be
    /// referenced in CEL expressions (e.g., `cel.MyStruct`).
    pub fn new(name: String) -> Self {
        Self {
            r#type: Type::new_struct(name),
            fields: Default::default(),
            defaults: Default::default(),
        }
    }

    /// Adds a field to the struct definition.
    ///
    /// This method adds a field with the given name and type. When the struct is
    /// instantiated in a CEL expression, this field must be provided unless it
    /// has a default value (see [`add_field_with_default`](Self::add_field_with_default)).
    pub fn add_field(self, field: String, t: Type) -> Self {
        self.insert_field(field, t, None)
    }

    /// Adds a field to the struct definition with a default value.
    ///
    /// This method adds a field with the given name and a default value. The type
    /// of the field is automatically inferred from the default value. When the
    /// struct is instantiated in a CEL expression, this field may be omitted, in
    /// which case the default value will be used.
    pub fn add_field_with_default(self, field: String, default: Box<dyn Val>) -> Self {
        self.insert_field(field, default.get_type().to_owned(), Some(default))
    }

    /// Internal method to insert a field into the struct definition.
    fn insert_field(self, field: String, t: Type, default: Option<Box<dyn Val>>) -> Self {
        let mut def = self;
        def.fields.insert(field.clone(), t);
        if let Some(default) = default {
            def.defaults.insert(field, default);
        }
        def
    }

    /// Creates a new instance of the struct with the given field values.
    ///
    /// This method is used internally by the CEL execution engine to instantiate
    /// a struct from a CEL expression.
    ///
    /// # Errors
    ///
    /// Missing fields will be populated with their default values if defined.
    /// Returns an error if:
    /// - A field is missing and has no default value.
    /// - A field's type does not match the type in the definition.
    /// - An unknown field name is provided.
    fn new_struct<'b, 'v>(
        &self,
        fields: BTreeMap<String, CowVal<'b, 'v>>,
    ) -> Result<CelStruct<'v>, ExecutionError> {
        let name = self.r#type.name();
        let mut s = CelStruct::new(name.to_owned());
        let mut fields = fields;
        for (field, default) in &self.defaults {
            if let Some(value) = fields.remove(field) {
                s.add_field_value(field.clone(), value);
            } else {
                s.add_field_value(field.clone(), CowVal::Owned(default.clone_as_boxed()));
            }
        }
        for (field, value) in fields {
            match self.fields.get(&field) {
                Some(t) => {
                    if t != value.get_type() {
                        return Err(ExecutionError::UnexpectedType {
                            got: value.get_type().name().to_owned(),
                            want: format!("{} for field {field} in {name}", t.name()),
                        });
                    }
                    s.add_field_value(field, value);
                }
                None => {
                    return Err(ExecutionError::NoSuchKey(std::sync::Arc::new(format!(
                        "field `{field}` on struct `{name}`"
                    ))))
                }
            }
        }
        Ok(s)
    }
}

#[cfg(feature = "structs")]
impl StructType for StructDef {
    fn get_type(&self) -> &Type {
        &self.r#type
    }

    fn new_value<'b, 'v>(
        &self,
        fields: BTreeMap<String, CowVal<'b, 'v>>,
    ) -> Result<Box<dyn Val + 'v>, ExecutionError> {
        Ok(Box::new(self.new_struct(fields)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::value::Val;
    use std::sync::Arc;

    #[test]
    fn test_env_default() {
        let _: Arc<dyn Send + Sync> = Arc::new(Env::default());
    }

    #[test]
    fn the_standard_library_registers_its_types() {
        let names = [
            "bool",
            "bytes",
            "double",
            "int",
            "list",
            "map",
            "null_type",
            "optional_type",
            "string",
            "type",
            "uint",
            #[cfg(feature = "chrono")]
            "google.protobuf.Duration",
            #[cfg(feature = "chrono")]
            "google.protobuf.Timestamp",
        ];
        let stdlib = Env::stdlib();
        let default = Env::default();
        for name in names {
            assert_eq!(stdlib.types().find_type(name).map(Type::name), Some(name));
            assert!(default.types().find_type(name).is_none(), "{name}");
        }
    }

    #[test]
    fn add_type_rejects_another_type_of_a_registered_name() {
        let mut env = Env::stdlib();
        assert_eq!(
            env.add_type(Type::new_opaque_type("optional_type")),
            Err(DeclarationError::type_conflict("optional_type"))
        );
        assert_eq!(env.add_type(types::OPTIONAL_TYPE), Ok(()));
    }

    fn noop<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, crate::ExecutionError> {
        Ok(args.into_iter().next().unwrap())
    }

    fn duplicate(function: &str, id: &str) -> DeclarationError {
        DeclarationError::duplicate_overload(function, id)
    }

    #[test]
    fn add_overload_rejects_a_duplicate_id() {
        let mut env = Env::default();
        assert_eq!(
            env.add_overload("f", "f_int", vec![types::INT_TYPE], noop),
            Ok(())
        );
        // another signature, but the id is taken
        assert_eq!(
            env.add_overload("f", "f_int", vec![types::STRING_TYPE], noop),
            Err(duplicate("f", "f_int"))
        );
    }

    #[test]
    fn add_overload_rejects_a_duplicate_signature() {
        let mut env = Env::default();
        assert_eq!(
            env.add_overload("f", "f_int", vec![types::INT_TYPE], noop),
            Ok(())
        );
        // another id, but the signature is taken
        assert_eq!(
            env.add_overload("f", "other_id", vec![types::INT_TYPE], noop),
            Err(duplicate("f", "other_id"))
        );
    }

    #[test]
    fn add_member_overload_rejects_a_duplicate_id_or_signature() {
        let mut env = Env::default();
        assert_eq!(
            env.add_member_overload("f", "int_f", types::INT_TYPE, vec![], noop),
            Ok(())
        );
        assert_eq!(
            env.add_member_overload("f", "int_f", types::STRING_TYPE, vec![], noop),
            Err(duplicate("f", "int_f"))
        );
        assert_eq!(
            env.add_member_overload("f", "other_id", types::INT_TYPE, vec![], noop),
            Err(duplicate("f", "other_id"))
        );
    }

    /// An id is unique across the global and member overloads of a function,
    /// while a signature also includes whether the overload is a member: `f(int)`
    /// and `int.f()` are different overloads, but may not share an id.
    #[test]
    fn a_global_and_a_member_overload_may_share_a_shape_but_not_an_id() {
        let mut env = Env::default();
        assert_eq!(
            env.add_overload("f", "f_int", vec![types::INT_TYPE], noop),
            Ok(())
        );
        assert_eq!(
            env.add_member_overload("f", "int_f", types::INT_TYPE, vec![], noop),
            Ok(())
        );
        assert_eq!(
            env.add_member_overload("f", "f_int", types::STRING_TYPE, vec![], noop),
            Err(duplicate("f", "f_int"))
        );
    }

    /// The same id is fine on another function, and a rejected overload leaves the
    /// environment as it was.
    #[test]
    fn a_rejected_overload_is_not_declared() {
        let mut env = Env::default();
        env.add_overload("f", "f_int", vec![types::INT_TYPE], noop)
            .unwrap();
        assert!(env
            .add_overload("f", "f_dup", vec![types::INT_TYPE], noop)
            .is_err());
        assert!(env
            .add_overload("g", "f_int", vec![types::INT_TYPE], noop)
            .is_ok());

        let int: Box<dyn Val> = Box::new(crate::common::types::CelInt::from(1));
        assert!(env.find_overload("f", &[CowVal::Owned(int)]).is_some());
        assert!(env.has_overload("f") && env.has_overload("g"));
        assert!(!env.has_member_overload("f"));
    }
}
