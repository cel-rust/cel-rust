use crate::common::{
    decls::FunctionDecl,
    functions::{Function, Op},
    types::{self, Type},
    value::CowVal,
};
use crate::container::Container;
use crate::parser::{Macro, Macros, Parser};
use crate::registry::{TypeDecl, TypeRegistry};
use crate::{common::types::CelStruct, common::value::Val, ExecutionError, StructType};
use crate::{DeclarationError, Library, ParseErrors, Program};
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
/// use cel::{Env, StructDef, common::types, common::types::CelString};
///
/// let mut env = Env::stdlib();
/// env.add_type(
///     StructDef::new("cel.MyStruct".to_owned())
///         .add_field("some_field".to_owned(), types::STRING_TYPE)
///         .add_field_with_default("with_default".to_owned(), Box::new(CelString::from("default_value")))
/// ).unwrap();
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
    optional: bool,
    container: Container,
    libraries: BTreeSet<String>,
}

/// The name the standard library is added under, as in cel-go.
const STANDARD_LIBRARY: &str = "cel.lib.std";

/// The name the `optional` library is added under, as in cel-go.
const OPTIONAL_LIBRARY: &str = "cel.lib.optional";

impl Default for Env {
    fn default() -> Self {
        Env {
            functions: BTreeMap::new(),
            namespaces: BTreeSet::new(),
            types: TypeRegistry::default(),
            macros: Arc::default(),
            error_on_duplicate_map_keys: true,
            optional: true,
            container: Container::default(),
            libraries: BTreeSet::new(),
        }
    }
}

impl Env {
    /// Returns the standard library environment.
    ///
    /// This environment contains all the standard functions and types as defined by the
    /// CEL specification.
    pub fn stdlib() -> Env {
        Env {
            macros: Macros::standard(),
            ..Default::default()
        }
        .with_stdlib()
    }

    /// Registers the standard types and functions, as defined by the CEL
    /// specification, on this environment and returns it.
    ///
    /// Unlike [`Env::stdlib`], it adds no macros: on an [`Env::default`],
    /// `has`, `all`, `exists`, `exists_one`, `map` and `filter` are left as
    /// the calls they are written as.
    ///
    /// The `duration` and `timestamp` types and functions need the `chrono`
    /// feature; the `optional` ones are only registered while this
    /// environment supports optional values, as it does by default: see
    /// [`Env::with_optional_support`].
    ///
    /// They are added as the libraries `cel.lib.std` and `cel.lib.optional`,
    /// see [`Env::has_library`], once each: calling this on an
    /// [`Env::stdlib`], or twice, registers nothing again.
    ///
    /// # Panics
    ///
    /// Panics if one of the standard overloads was declared some other way,
    /// as with [`Env::add_overload`], or if another type is already
    /// registered under one of the standard types' names.
    ///
    /// # Example
    ///
    /// ```
    /// use cel::{Context, Env, Value};
    /// use std::sync::Arc;
    ///
    /// let env = Env::default().with_stdlib();
    /// let program = env.compile("size('abc') == 3").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    /// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
    /// ```
    pub fn with_stdlib(mut self) -> Self {
        if self.libraries.insert(STANDARD_LIBRARY.to_owned()) {
            types::bool::stdlib(&mut self);
            types::bytes::stdlib(&mut self);
            types::double::stdlib(&mut self);
            types::r#dyn::stdlib(&mut self);
            types::int::stdlib(&mut self);
            types::list::stdlib(&mut self);
            types::map::stdlib(&mut self);
            types::null::stdlib(&mut self);
            types::string::stdlib(&mut self);
            types::type_val::stdlib(&mut self);
            types::uint::stdlib(&mut self);

            #[cfg(feature = "chrono")]
            {
                types::duration::stdlib(&mut self);
                types::timestamp::stdlib(&mut self);
            }
        }
        if self.optional && self.libraries.insert(OPTIONAL_LIBRARY.to_owned()) {
            types::optional::stdlib(&mut self);
        }
        self
    }

    /// Sets whether this environment supports optional values, as it does by
    /// default, and returns it.
    ///
    /// Without, the parsers it builds with [`Env::parser`], and so
    /// [`Env::compile`], reject the optional syntax (`a.?b`, `a[?b]`, `[?a]`
    /// and `{?k: v}`) as unsupported, and [`Env::with_stdlib`] leaves out the
    /// `optional` library (`optional.of`, `optional.none`, `hasValue`, ...).
    ///
    /// That library is registered when [`Env::with_stdlib`] runs: disable
    /// support before calling it. On an [`Env::stdlib`], or after
    /// [`Env::with_stdlib`], this only disables the syntax.
    ///
    /// # Example
    ///
    /// ```
    /// use cel::{Context, Env};
    /// use std::sync::Arc;
    ///
    /// let env = Env::default().with_optional_support(false).with_stdlib();
    /// assert!(env.compile("{'a': 1}.?a").is_err());
    ///
    /// let program = env.compile("optional.of(1).hasValue()").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    /// assert!(program.execute(&context).is_err());
    /// ```
    pub fn with_optional_support(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
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
        Parser::new()
            .enable_optional_syntax(self.optional)
            .with_macros(Arc::clone(&self.macros))
    }

    /// Compiles `source` into a [`Program`], expanding the macros of this
    /// environment: the standard ones of [`Env::stdlib`], and those added with
    /// [`Env::add_macro`], by extensions included.
    ///
    /// Macros are expanded while compiling: the program doesn't depend on the
    /// environment afterwards.
    ///
    /// # Errors
    ///
    /// Fails with the [`ParseErrors`] of `source`, a macro's included.
    ///
    /// # Example
    ///
    /// ```
    /// use cel::{Context, Env, Value};
    /// use std::sync::Arc;
    ///
    /// let env = Env::stdlib();
    /// let program = env.compile("[1, 2, 3].exists(x, x > 2)").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    /// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
    /// ```
    pub fn compile(&self, source: &str) -> Result<Program, ParseErrors> {
        Program::parse_with(self.parser(), source)
    }

    /// Adds a macro, expanded by the programs this environment compiles with
    /// [`Env::compile`], and by the parsers it builds with [`Env::parser`];
    /// [`Program::compile`] only expands the standard ones.
    ///
    /// # Errors
    ///
    /// Fails with [`DeclarationError::DuplicateMacro`] if a macro for the same
    /// function, called the same way (globally or on a target) with as many
    /// arguments, or with any number of them, is already added: the standard
    /// ones of [`Env::stdlib`] included. A macro for any number of arguments
    /// and one for an exact number coexist: the latter wins for its calls.
    ///
    /// # Example
    ///
    /// ```
    /// use cel::common::ast::{operators, CallExpr, Expr};
    /// use cel::parser::Macro;
    /// use cel::{Context, Env, Value};
    /// use std::sync::Arc;
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
    /// let program = env.compile("implies(false, 1 / 0 == 1)").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    /// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
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
        self.declare_overload(name, id, args, Op::Function(op))
    }

    /// Adds a global function overload, as [`Env::add_overload`] does, that a
    /// closure implements: one that can capture, say the settings of the
    /// [`Library`] declaring it.
    ///
    /// Write the closure in the call, as below, or return it from a function
    /// declared to return `impl for<'b, 'v> Fn(..) -> ..`: one bound to a
    /// variable first isn't inferred to take arguments of any lifetimes.
    ///
    /// ```
    /// use cel::{Context, Env, Value, common::types, common::value::CowVal};
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// let factor = 3;
    /// env.add_overload_with("scale", "scale_int", vec![types::INT_TYPE], move |args| {
    ///     let val = args[0].downcast_ref::<types::CelInt>().unwrap();
    ///     Ok(CowVal::owned(types::CelInt::from(val.inner() * factor)))
    /// }).unwrap();
    ///
    /// let program = env.compile("scale(2)").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    /// assert_eq!(program.execute(&context), Ok(Value::Int(6)));
    /// ```
    ///
    /// # Errors
    ///
    /// Fails as [`Env::add_overload`] does.
    pub fn add_overload_with<F>(
        &mut self,
        name: &str,
        id: &str,
        args: Vec<types::Type>,
        op: F,
    ) -> Result<(), DeclarationError>
    where
        F: for<'b, 'v> Fn(Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError>
            + Send
            + Sync
            + std::panic::UnwindSafe
            + std::panic::RefUnwindSafe
            + 'static,
    {
        self.declare_overload(name, id, args, Op::Closure(Box::new(op)))
    }

    fn declare_overload(
        &mut self,
        name: &str,
        id: &str,
        args: Vec<types::Type>,
        op: Op,
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
    ///
    /// `None` as well when that overload is implemented by a closure, added
    /// with [`Env::add_overload_with`]: it has no [`Function`] to return.
    pub fn find_overload(&self, name: &str, args: &[CowVal<'_, '_>]) -> Option<Function> {
        match self.functions.get(name) {
            None => None,
            Some(fn_decl) => fn_decl.find_overload(false, args),
        }
    }

    /// The implementation of the overload a call of `name` with `args`
    /// resolves to: a member overload's if `member_function`, with the target
    /// as the first of `args`.
    pub(crate) fn find_op(
        &self,
        name: &str,
        member_function: bool,
        args: &[CowVal<'_, '_>],
    ) -> Option<&Op> {
        self.functions.get(name)?.find_op(member_function, args)
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
        let op = Op::Function(op);
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
    /// use cel::{Context, Env, Value};
    /// use cel::common::types::Type;
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.add_type(Type::new_opaque_type("Ip")).unwrap();
    /// let program = env.compile("type(Ip) == type").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    ///
    /// assert_eq!(program.execute(&context), Ok(Value::Bool(true)));
    /// ```
    ///
    /// Registering a `StructType`, e.g. a `StructDef`, also lets struct
    /// literals construct it:
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
    /// use cel::{Context, Env, Value};
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.set_container("x").unwrap();
    /// let program = env.compile("y").unwrap();
    /// let mut context = Context::with_env(Arc::new(env));
    /// context.add_variable_from_value("x.y", true);
    ///
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
    /// handing it this environment to register its types, overloads and
    /// macros on. Compile with [`Env::compile`] for the macros to be expanded.
    ///
    /// The extensions of this crate add a [`Library`], with its default
    /// settings: adding one again changes nothing. To add one with settings
    /// of its own, see [`Env::add_library`].
    ///
    /// ```
    /// use cel::{extensions, Context, Env, Value};
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.add_extension(extensions::strings);
    /// let program = env.compile("'TacoCat'.lowerAscii()").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    ///
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

    /// Adds a [`Library`], by handing it this environment to register its
    /// types, overloads and macros on: this is how to add an extension with
    /// settings of its own, when [`Env::add_extension`] adds it with its
    /// default ones.
    ///
    /// A library with a [name](Library::name) is added once: adding one of a
    /// name that [is there already](Env::has_library) changes nothing,
    /// whatever its settings.
    ///
    /// ```
    /// use cel::{extensions, Env};
    ///
    /// let mut env = Env::stdlib();
    /// env.add_library(extensions::Strings::default()).unwrap();
    /// assert!(env.has_library("cel.lib.ext.strings"));
    /// // The strings library is there already: nothing is registered again.
    /// env.add_extension(extensions::strings).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with the [`DeclarationError`] the library failed to register
    /// with, as when this environment already declares one of its overloads.
    /// What the library registered until then stays, but the library isn't
    /// added: this environment doesn't [have](Env::has_library) its name.
    pub fn add_library(&mut self, library: impl Library) -> Result<(), DeclarationError> {
        let Some(name) = library.name() else {
            return library.register(self);
        };
        // Added before it registers, as in cel-go: a library that is added
        // again while it registers, by one it adds, is there already.
        if !self.libraries.insert(name.to_owned()) {
            return Ok(());
        }
        library.register(self).inspect_err(|_| {
            self.libraries.remove(name);
        })
    }

    /// Whether a library of that [name](Library::name) was added to this
    /// environment. [`Env::with_stdlib`] adds the standard library as
    /// `cel.lib.std`, and the `optional` one as `cel.lib.optional`.
    ///
    /// ```
    /// use cel::{extensions, Env};
    ///
    /// let mut env = Env::stdlib();
    /// assert!(env.has_library("cel.lib.std"));
    /// assert!(!env.has_library("cel.lib.ext.strings"));
    /// env.add_extension(extensions::strings).unwrap();
    /// assert!(env.has_library("cel.lib.ext.strings"));
    /// ```
    pub fn has_library(&self, name: &str) -> bool {
        self.libraries.contains(name)
    }

    /// The names of the libraries added to this environment, sorted.
    pub fn libraries(&self) -> impl Iterator<Item = &str> {
        self.libraries.iter().map(String::as_str)
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
    /// use cel::{Context, Env, Value};
    /// use std::sync::Arc;
    ///
    /// let mut env = Env::stdlib();
    /// env.set_error_on_duplicate_map_keys(false);
    /// let program = env.compile("{'a': 1, 'a': 2}['a']").unwrap();
    /// let context = Context::with_env(Arc::new(env));
    ///
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
pub struct StructDef {
    r#type: Type,
    fields: BTreeMap<String, Type>,
    defaults: BTreeMap<String, Box<dyn Val>>,
}

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
    use std::cell::Cell;
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
    fn a_macro_expands_only_the_calls_of_its_style_and_argument_count() {
        use crate::common::ast::Expr;

        let mut env = Env::default();
        env.add_macro(Macro::receiver("f", 1, first_arg)).unwrap();
        let expanded = env.parser().parse("m.f(1)").unwrap();
        assert!(matches!(expanded.expr, Expr::Literal(_)), "{expanded:?}");
        for source in ["m.f()", "m.f(1, 2)", "f(1)"] {
            let as_written = env.parser().parse(source).unwrap();
            assert!(
                matches!(as_written.expr, Expr::Call(_)),
                "{source}: {as_written:?}"
            );
        }
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
        // For any number of arguments, it's yet other calls, once.
        assert_eq!(
            env.add_macro(Macro::receiver_var_arg("exists", first_arg)),
            Ok(())
        );
        assert_eq!(
            env.add_macro(Macro::receiver_var_arg("exists", first_arg)),
            Err(DeclarationError::duplicate_macro("exists"))
        );
        assert_eq!(
            env.add_macro(Macro::global_var_arg("exists", first_arg)),
            Ok(())
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

    /// A library, `acme.lib.counted`, that counts the times it registers.
    struct Counted<'a>(&'a Cell<u32>);

    impl Library for Counted<'_> {
        fn name(&self) -> Option<&str> {
            Some("acme.lib.counted")
        }

        fn register(&self, _: &mut Env) -> Result<(), DeclarationError> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn a_named_library_is_registered_once() {
        let registered = Cell::new(0);
        let mut env = Env::default();
        assert!(!env.has_library("acme.lib.counted"));
        assert_eq!(env.add_library(Counted(&registered)), Ok(()));
        assert_eq!(env.add_library(Counted(&registered)), Ok(()));
        assert_eq!(registered.get(), 1);
        assert!(env.has_library("acme.lib.counted"));
        assert!(!env.has_library("acme.lib"));
    }

    /// A library is its name: another one of an added name isn't registered.
    #[test]
    fn a_library_of_an_added_name_is_not_registered() {
        struct DeclaresF;

        impl Library for DeclaresF {
            fn name(&self) -> Option<&str> {
                Some("acme.lib.counted")
            }

            fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
                env.add_overload("f", "f", vec![], noop)
            }
        }

        let registered = Cell::new(0);
        let mut env = Env::default();
        assert_eq!(env.add_library(Counted(&registered)), Ok(()));
        assert_eq!(env.add_library(DeclaresF), Ok(()));
        assert!(!env.has_overload("f"));
        assert_eq!(env.libraries().collect::<Vec<_>>(), ["acme.lib.counted"]);
    }

    #[test]
    fn a_library_without_a_name_is_registered_every_time() {
        struct Unnamed<'a>(&'a Cell<u32>);

        impl Library for Unnamed<'_> {
            fn register(&self, _: &mut Env) -> Result<(), DeclarationError> {
                self.0.set(self.0.get() + 1);
                Ok(())
            }
        }

        let registered = Cell::new(0);
        assert_eq!(Unnamed(&registered).name(), None);
        let mut env = Env::default();
        for _ in 0..2 {
            assert_eq!(env.add_library(Unnamed(&registered)), Ok(()));
        }
        assert_eq!(registered.get(), 2);
        assert_eq!(env.libraries().count(), 0);
    }

    /// An extension is a function, called every time it is added: it is only
    /// added once when it adds a library.
    #[test]
    fn an_extension_is_called_every_time_it_is_added() {
        fn declares_f(env: &mut Env) -> Result<(), DeclarationError> {
            env.add_overload("f", "f", vec![], noop)
        }

        fn adds_a_library(env: &mut Env) -> Result<(), DeclarationError> {
            struct DeclaresG;

            impl Library for DeclaresG {
                fn name(&self) -> Option<&str> {
                    Some("acme.lib.g")
                }

                fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
                    env.add_overload("g", "g", vec![], noop)
                }
            }

            env.add_library(DeclaresG)
        }

        let mut env = Env::default();
        assert_eq!(env.add_extension(declares_f), Ok(()));
        assert_eq!(env.add_extension(declares_f), Err(duplicate("f", "f")));
        assert_eq!(env.add_extension(adds_a_library), Ok(()));
        assert_eq!(env.add_extension(adds_a_library), Ok(()));
        assert!(env.has_overload("g"));
        assert_eq!(env.libraries().collect::<Vec<_>>(), ["acme.lib.g"]);
    }

    #[test]
    fn a_library_that_fails_to_register_is_not_added() {
        struct Failing<'a>(&'a Cell<u32>);

        impl Library for Failing<'_> {
            fn name(&self) -> Option<&str> {
                Some("acme.lib.failing")
            }

            fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
                self.0.set(self.0.get() + 1);
                env.set_container("not a container")
            }
        }

        let registered = Cell::new(0);
        let mut env = Env::default();
        for attempts in 1..=2 {
            assert_eq!(
                env.add_library(Failing(&registered)),
                Err(DeclarationError::invalid_container_name("not a container"))
            );
            assert_eq!(registered.get(), attempts);
            assert!(!env.has_library("acme.lib.failing"));
        }
    }

    /// As in cel-go, a library is added before it registers: when a library
    /// it adds, or itself, adds it again, it is there already.
    #[test]
    fn a_library_is_added_by_the_time_it_registers() {
        struct AddsItself<'a>(&'a Cell<u32>);

        impl Library for AddsItself<'_> {
            fn name(&self) -> Option<&str> {
                Some("acme.lib.recursive")
            }

            fn register(&self, env: &mut Env) -> Result<(), DeclarationError> {
                self.0.set(self.0.get() + 1);
                assert!(env.has_library("acme.lib.recursive"));
                env.add_library(AddsItself(self.0))
            }
        }

        let registered = Cell::new(0);
        let mut env = Env::default();
        assert_eq!(env.add_library(AddsItself(&registered)), Ok(()));
        assert_eq!(registered.get(), 1);
        assert!(env.has_library("acme.lib.recursive"));
    }

    #[test]
    fn the_standard_library_is_added_once() {
        use crate::{Context, Value};

        assert_eq!(Env::default().libraries().count(), 0);
        // Registered again, its overloads would be duplicates.
        for env in [
            Env::stdlib(),
            Env::stdlib().with_stdlib(),
            Env::default().with_stdlib().with_stdlib(),
        ] {
            assert_eq!(
                env.libraries().collect::<Vec<_>>(),
                ["cel.lib.optional", "cel.lib.std"]
            );
            let program = env.compile("optional.of(size('abc')).value()").unwrap();
            let context = Context::with_env(Arc::new(env));
            assert_eq!(program.execute(&context), Ok(Value::Int(3)));
        }
    }

    /// The `optional` library is one of its own: the standard library adds
    /// it when optional values are supported, even if only later.
    #[test]
    fn the_optional_library_is_added_once_optional_values_are_supported() {
        let env = Env::default().with_optional_support(false).with_stdlib();
        assert_eq!(env.libraries().collect::<Vec<_>>(), ["cel.lib.std"]);
        assert!(!env.has_overload("optional.of"));

        let env = env.with_optional_support(true).with_stdlib().with_stdlib();
        assert!(env.has_library("cel.lib.optional"));
        assert!(env.has_overload("optional.of"));
    }

    /// An extension adds its library, with its default settings.
    #[test]
    fn the_extensions_are_added_once_under_the_names_of_cel_go() {
        use crate::extensions;

        type Extension = fn(&mut Env) -> Result<(), DeclarationError>;
        let named: &[(&str, Extension)] = &[
            ("cel.lib.ext.cel.bindings", extensions::bindings),
            #[cfg(feature = "ext_encoders")]
            ("cel.lib.ext.encoders", extensions::encoders),
            ("cel.lib.ext.lists", extensions::lists),
            ("cel.lib.ext.math", extensions::math),
            ("cel.lib.ext.network", extensions::network),
            ("cel.lib.ext.strings", extensions::strings),
        ];
        let mut env = Env::stdlib();
        for (name, extension) in named {
            assert!(!env.has_library(name), "{name}");
            for _ in 0..2 {
                assert_eq!(env.add_extension(extension), Ok(()), "{name}");
            }
            assert!(env.has_library(name), "{name}");
        }
        let names = named.iter().map(|(name, _)| *name);
        assert_eq!(
            env.libraries().collect::<Vec<_>>(),
            names
                .chain(["cel.lib.optional", "cel.lib.std"])
                .collect::<Vec<_>>()
        );

        assert_eq!(env.add_library(extensions::Strings::default()), Ok(()));
        let mut env = Env::stdlib();
        assert_eq!(env.add_library(extensions::Strings::default()), Ok(()));
        assert_eq!(env.add_extension(extensions::strings), Ok(()));
        assert!(env.has_member_overload("lowerAscii"));
    }

    /// A closure holds what it captured, and is called as a function is.
    #[test]
    fn an_overload_can_be_a_closure() {
        use crate::common::types::CelInt;
        use crate::{Context, Value};

        fn scale(
            factor: i64,
        ) -> impl for<'b, 'v> Fn(Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError>
        {
            move |args| {
                let n = args[0].downcast_ref::<CelInt>().unwrap().inner();
                Ok(CowVal::owned(CelInt::from(n * factor)))
            }
        }

        let mut env = Env::stdlib();
        for (name, factor) in [("twice", 2), ("a.thrice", 3)] {
            assert_eq!(
                env.add_overload_with(name, name, vec![types::INT_TYPE], scale(factor)),
                Ok(())
            );
        }
        assert!(env.has_namespace("a"));

        let env = Arc::new(env);
        for (expr, expected) in [("twice(4)", 8), ("a.thrice(4)", 12)] {
            let program = env.compile(expr).unwrap();
            let context = Context::with_env(env.clone());
            assert_eq!(program.execute(&context), Ok(Value::Int(expected)));
        }
    }

    /// A closure and a function are overloads alike: neither's id, nor
    /// signature, can be another's.
    #[test]
    fn add_overload_with_rejects_a_duplicate_id_or_signature() {
        let mut env = Env::default();
        assert_eq!(
            env.add_overload("f", "f_int", vec![types::INT_TYPE], noop),
            Ok(())
        );
        assert_eq!(
            env.add_overload_with("f", "f_int", vec![types::STRING_TYPE], noop),
            Err(duplicate("f", "f_int"))
        );
        assert_eq!(
            env.add_overload_with("f", "other_id", vec![types::INT_TYPE], noop),
            Err(duplicate("f", "other_id"))
        );
        assert_eq!(
            env.add_overload_with("f", "f_string", vec![types::STRING_TYPE], noop),
            Ok(())
        );
        assert_eq!(
            env.add_overload("f", "f_string", vec![types::BOOL_TYPE], noop),
            Err(duplicate("f", "f_string"))
        );
    }

    /// A closure isn't a [`Function`]: `find_overload` has none to hand out
    /// for the overload, which a call still resolves to.
    #[test]
    fn find_overload_only_finds_the_functions() {
        use crate::common::types::{CelInt, CelString};

        let mut env = Env::default();
        env.add_overload("f", "f_int", vec![types::INT_TYPE], noop)
            .unwrap();
        env.add_overload_with("f", "f_string", vec![types::STRING_TYPE], noop)
            .unwrap();

        let int = || vec![CowVal::owned(CelInt::from(1))];
        let string = || vec![CowVal::owned(CelString::from("a"))];
        assert!(env.find_overload("f", &int()).is_some());
        assert!(env.find_overload("f", &string()).is_none());
        for args in [int, string] {
            let op = env.find_op("f", false, &args()).unwrap();
            assert!(op.call(args()).is_ok());
            assert!(env.find_op("f", true, &args()).is_none());
        }
        assert!(env.find_op("g", false, &int()).is_none());
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
