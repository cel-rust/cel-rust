use crate::common::{
    decls::FunctionDecl,
    functions::Function,
    types::{self, Type},
    value::CowVal,
};
use crate::container::Container;
use crate::parser::{Macro, Macros, Parser};
use crate::registry::{TypeDecl, TypeRegistry};
use crate::DeclarationError;
#[cfg(feature = "structs")]
use crate::{common::types::CelStruct, common::value::Val, ExecutionError, StructType};
use std::collections::{
    btree_map::Entry::{Occupied, Vacant},
    BTreeMap, BTreeSet,
};
use std::sync::Arc;

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
/// env.add_type(
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
    namespaces: BTreeSet<String>,
    types: TypeRegistry,
    macros: Arc<Macros>,
    error_on_duplicate_map_keys: bool,
    container: Container,
}

impl Default for Env {
    fn default() -> Self {
        Env {
            functions: BTreeMap::new(),
            namespaces: BTreeSet::new(),
            types: TypeRegistry::default(),
            macros: Arc::default(),
            error_on_duplicate_map_keys: true,
            container: Container::default(),
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
        env.macros = Macros::standard();
        env
    }

    /// Returns a parser that expands the macros of this environment.
    ///
    /// [`Env::stdlib`] has the standard macros (`has`, `all`, `exists`,
    /// `exists_one`, `map` and `filter`); an [`Env::default`] has none, so its
    /// parser leaves such calls as they are written.
    ///
    /// # Example
    ///
    /// ```
    /// use cel::{Context, Env, Value};
    ///
    /// let env = Env::stdlib();
    /// let expr = env.parser().parse("[1, 2, 3].exists(x, x > 2)").unwrap();
    /// assert_eq!(Value::resolve(&expr, &Context::default()), Ok(Value::Bool(true)));
    /// ```
    pub fn parser(&self) -> Parser {
        Parser::new().with_macros(Arc::clone(&self.macros))
    }

    /// Adds a macro, expanded by the parsers this environment builds with
    /// [`Env::parser`].
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::DuplicateMacro`] if a macro for the same
    /// function, called the same way (globally or on a target) with as many
    /// arguments, is already added: the standard ones of [`Env::stdlib`]
    /// included.
    ///
    /// # Example
    ///
    /// ```
    /// use cel::common::ast::{operators, CallExpr, Expr};
    /// use cel::parser::Macro;
    /// use cel::{Context, Env, Value};
    ///
    /// // `implies(a, b)` is parsed as `!a || b`: true whenever `a` is false,
    /// // whatever `b` evaluates to.
    /// let implies = Macro::global("implies", 2, |helper, _target, args| {
    ///     let b = args.pop().unwrap();
    ///     let a = args.pop().unwrap();
    ///     let not_a = helper.next_expr(Expr::Call(CallExpr {
    ///         func_name: operators::LOGICAL_NOT.to_string(),
    ///         target: None,
    ///         args: vec![a],
    ///     }));
    ///     Ok(Some(helper.next_expr(Expr::Call(CallExpr {
    ///         func_name: operators::LOGICAL_OR.to_string(),
    ///         target: None,
    ///         args: vec![not_a, b],
    ///     }))))
    /// });
    ///
    /// let mut env = Env::stdlib();
    /// env.add_macro(implies).unwrap();
    /// let expr = env.parser().parse("implies(false, 1 / 0 == 1)").unwrap();
    /// assert_eq!(Value::resolve(&expr, &Context::default()), Ok(Value::Bool(true)));
    /// ```
    pub fn add_macro(&mut self, m: Macro) -> Result<(), DeclarationError> {
        // The standard macros are shared by every `Env::stdlib()` and default
        // parser: the first macro added copies them for this environment.
        Arc::make_mut(&mut self.macros).add(m)
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
                if let Some((namespace, _)) = name.split_once('.') {
                    self.namespaces.insert(namespace.to_owned());
                }
                Ok(())
            }
            Occupied(occupied_entry) => {
                occupied_entry
                    .into_mut()
                    .add_overload(id.to_string(), false, args, op)
            }
        }
    }

    pub(crate) fn has_namespace(&self, namespace: &str) -> bool {
        self.namespaces.contains(namespace)
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
    /// to it by name, and, for a struct type, construct it.
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
    /// With the `structs` feature, registering a `StructType`, e.g. a
    /// `StructDef`, also lets struct literals construct it:
    /// `cel.MyStruct{some_field: 'value'}`.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::TypeConflict`] if another type, or
    /// another struct type, is already registered under that name, and with
    /// [`DeclarationError::InvalidTypeName`] if the name is not an identifier,
    /// or several separated by dots. Registering an equal type again is fine,
    /// and so is registering a struct type whose type was registered alone.
    pub fn add_type(&mut self, t: impl Into<TypeDecl>) -> Result<(), DeclarationError> {
        self.types.register(t)
    }

    /// Sets the container names are resolved against: with container `x`, an
    /// identifier, qualified identifier, struct type name or function name
    /// `y` also resolves to `x.y`, ahead of the plain `y`. In container `a.b`,
    /// `y` is tried as `a.b.y`, `a.y`, then `y`; a leading dot, as in `.y`,
    /// makes the name absolute, so only `y` is tried.
    ///
    /// Each call replaces the container; an empty name clears it.
    ///
    /// ```
    /// use cel::{Context, Env, Program, Value};
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.set_container("x").unwrap();
    /// let mut context = Context::with_env(Arc::new(env));
    /// context.add_variable_from_value("x.y", true);
    ///
    /// let program = Program::compile("y").unwrap();
    /// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::InvalidContainerName`] if `name` is
    /// neither empty nor an identifier, or several separated by dots; a
    /// leading dot is rejected.
    pub fn set_container(&mut self, name: &str) -> Result<(), DeclarationError> {
        self.container = Container::new(name)?;
        Ok(())
    }

    /// The name of the container, empty if none is set.
    pub fn container_name(&self) -> &str {
        self.container.name()
    }

    pub(crate) fn container(&self) -> &Container {
        &self.container
    }

    /// The types registered with the environment.
    pub fn types(&self) -> &TypeRegistry {
        &self.types
    }

    /// Adds an extension library, such as [`extensions::strings`], by
    /// handing it this environment to register its types and overloads on.
    ///
    /// ```
    /// use cel::{extensions, Context, Env, Program, Value};
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.add_extension(extensions::strings);
    /// let context = Context::with_env(Arc::new(env));
    ///
    /// let program = Program::compile("'TacoCat'.lowerAscii()").unwrap();
    /// assert_eq!(program.execute(&context), Ok(Value::from("tacocat")));
    /// ```
    ///
    /// [`extensions::strings`]: crate::extensions::strings
    pub fn add_extension(
        &mut self,
        extension: impl FnOnce(&mut Env) -> Result<(), DeclarationError>,
    ) -> Result<(), DeclarationError> {
        extension(self)
    }

    /// Sets whether a map literal that repeats a key is an error.
    ///
    /// On by default, as the spec requires. Turning it off keeps the last entry
    /// instead, so `{'a': 1, 'a': 2}` evaluates to `{'a': 2}`, which is what
    /// cel-go does. Mirrors cel-java's `CelOptions.errorOnDuplicateMapKeys`,
    /// where the shipped default (`CelOptions.DEFAULT`) also errors.
    ///
    /// Numeric keys compare by value, as the spec requires, so `{0: 1, 0u: 2}`
    /// repeats a key too, which cel-java doesn't catch. With the check off, both
    /// entries are kept, as in cel-go.
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
/// env.add_type(
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
    fn an_empty_container_name_clears_the_container() {
        use crate::{Context, Program, Value};

        let program = Program::compile("y").unwrap();
        let run = |env: &Arc<Env>| {
            let mut context = Context::with_env(env.clone());
            context.add_variable_from_value("x.y", true);
            program.execute(&context)
        };

        let mut env = Arc::new(Env::stdlib());
        assert_eq!(env.container_name(), "");
        let env_mut = Arc::get_mut(&mut env).unwrap();
        env_mut.set_container("x").unwrap();
        assert!(env_mut.set_container(".x").is_err());
        assert_eq!(env.container_name(), "x", "a failed call changes nothing");
        assert_eq!(run(&env), Ok(Value::Bool(true)));

        Arc::get_mut(&mut env).unwrap().set_container("").unwrap();
        assert_eq!(env.container_name(), "");
        assert!(run(&env).is_err(), "`y` no longer means `x.y`");
    }

    #[test]
    fn its_parser_expands_the_macros_of_the_env_only() {
        use crate::common::ast::Expr;

        let source = "[1].exists(x, x > 0)";
        let expanded = Env::stdlib().parser().parse(source).unwrap();
        assert!(
            matches!(expanded.expr, Expr::Comprehension(_)),
            "{expanded:?}"
        );
        let as_written = Env::default().parser().parse(source).unwrap();
        assert!(matches!(as_written.expr, Expr::Call(_)), "{as_written:?}");
    }

    fn first_arg(
        _: &mut crate::parser::MacroExprHelper<'_>,
        _: &mut Option<crate::IdedExpr>,
        args: &mut Vec<crate::IdedExpr>,
    ) -> Result<Option<crate::IdedExpr>, crate::ParseError> {
        Ok(Some(args.remove(0)))
    }

    #[test]
    fn a_macro_for_the_calls_of_another_is_a_duplicate() {
        let mut env = Env::stdlib();
        assert_eq!(
            env.add_macro(Macro::receiver("exists", 2, first_arg)),
            Err(DeclarationError::duplicate_macro("exists")),
            "the standard `exists` expands those"
        );
        // Called globally, or with another argument count, it's another call.
        assert_eq!(env.add_macro(Macro::global("exists", 2, first_arg)), Ok(()));
        assert_eq!(
            env.add_macro(Macro::receiver("exists", 3, first_arg)),
            Ok(())
        );
        assert_eq!(
            env.add_macro(Macro::receiver("exists", 3, first_arg)),
            Err(DeclarationError::duplicate_macro("exists"))
        );
    }

    /// Declines `f(x)` after popping `x`, or `mem::take`-ing it.
    fn takes_then_declines(
        _: &mut crate::parser::MacroExprHelper<'_>,
        _: &mut Option<crate::IdedExpr>,
        args: &mut Vec<crate::IdedExpr>,
    ) -> Result<Option<crate::IdedExpr>, crate::ParseError> {
        match args[0].expr {
            crate::common::ast::Expr::Literal(_) => drop(args.pop()),
            _ => drop(std::mem::take(&mut args[0])),
        }
        Ok(None)
    }

    #[test]
    fn declining_a_call_after_taking_from_it_is_an_error() {
        let mut env = Env::stdlib();
        env.add_macro(Macro::global("f", 1, takes_then_declines))
            .unwrap();
        for source in ["f(1)", "f(a)"] {
            let errors = env.parser().parse(source).unwrap_err().errors;
            let messages: Vec<_> = errors.iter().map(|e| e.msg.as_str()).collect();
            assert_eq!(
                messages,
                ["macro 'f' declined the call after taking from it"],
                "{source}"
            );
        }
    }

    fn declines(
        _: &mut crate::parser::MacroExprHelper<'_>,
        _: &mut Option<crate::IdedExpr>,
        _: &mut Vec<crate::IdedExpr>,
    ) -> Result<Option<crate::IdedExpr>, crate::ParseError> {
        Ok(None)
    }

    /// The parser leaves a default node for an argument it failed to expand:
    /// that isn't something a declining macro took.
    #[test]
    fn declining_a_call_with_a_broken_argument_reports_only_that_argument() {
        let mut env = Env::stdlib();
        env.add_macro(Macro::global("f", 1, declines)).unwrap();
        let errors = env.parser().parse("f(has(1))").unwrap_err().errors;
        let messages: Vec<_> = errors.iter().map(|e| e.msg.as_str()).collect();
        assert_eq!(messages, ["invalid argument to has() macro"]);
    }

    #[test]
    fn a_qualified_overload_declares_its_namespace() {
        let mut env = Env::default();
        env.add_overload("a.b.f", "a_b_f", vec![], noop).unwrap();
        env.add_overload("g", "g", vec![], noop).unwrap();
        env.add_member_overload("c.m", "c_m", types::INT_TYPE, vec![], noop)
            .unwrap();
        assert!(env.has_namespace("a"));
        assert!(!env.has_namespace("a.b"));
        assert!(!env.has_namespace("g"));
        assert!(!env.has_namespace("c"), "member overloads aren't qualified");
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
