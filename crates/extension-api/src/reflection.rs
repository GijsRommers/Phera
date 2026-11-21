//! Cold declaration queries used by `new ReflectionClass(Foo::class)`.
//!
//! Extensions see owned snapshots and opaque initializer handles. The runtime
//! keeps ownership of linked classes, function bytecode and access checks.

use std::rc::{Rc, Weak};

use phera_value::{Object, PhpArray, Value};

use crate::{LocalFuture, NativeResult};

pub mod lazy;

#[derive(Clone, Debug, Default)]
pub struct SourceInfo {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub doc_comment: Option<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReflectedClassKind {
    Class,
    Interface,
    Trait,
    Enum,
}

#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub id: u32,
    pub name: String,
    pub kind: ReflectedClassKind,
    pub source: Option<SourceInfo>,
    pub extension: Option<String>,
    pub parent: Option<String>,
    pub interfaces: Vec<String>,
    pub traits: Vec<String>,
    pub enum_backing: Option<&'static str>,
    pub trait_aliases: Vec<(String, String)>,
    pub modifiers: i64,
    pub is_abstract: bool,
    pub instantiable: bool,
    pub cloneable: bool,
    pub iterable: bool,
    pub anonymous: bool,
    pub constructor: Option<String>,
    pub attributes: Vec<Rc<AttributeInfo>>,
}

/// A request-local handle to code that evaluates a declaration expression.
#[derive(Clone, Debug)]
pub struct Initializer {
    pub unit: u32,
    pub index: usize,
    pub scope: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AttributeInfo {
    pub validation_error: Option<String>,
    pub site: Option<AttributeSite>,
    pub name: String,
    pub arguments: Vec<(Option<String>, AttributeValue)>,
    pub target: AttributeTarget,
}

/// The constructor of an attribute sees the declaration as its PHP caller.
#[derive(Clone, Debug)]
pub struct AttributeSite {
    pub file: String,
    pub line: usize,
    pub strict_types: bool,
}

#[derive(Clone, Debug)]
pub enum AttributeValue {
    Expression(Initializer),
    Literal(Value),
}

pub use phera_source::attributes::{AttributeTarget, builtin_flags, target_names};

#[derive(Clone, Debug, Default)]
pub struct MemberInfo {
    pub name: String,
    pub class: String,
    pub modifiers: i64,
    pub lookup_class: String,
    pub source: Option<SourceInfo>,
    pub attributes: Vec<Rc<AttributeInfo>>,
    pub declared_type: Option<String>,
    pub enum_case: bool,
    pub promoted: bool,
    pub has_default: bool,
    pub dynamic: bool,
    pub property: Option<PropertyHandle>,
    /// For a method in a class's member list, where the VM keeps its
    /// reflected function (see [`FunctionTarget::Member`]).
    pub method: Option<MethodHandle>,
    /// For a plain instance property (no hooks, not virtual), where an
    /// object keeps it, for the direct methods of `ReflectionProperty`.
    pub slot_access: Option<SlotAccess>,
}

/// An instance property in an object's slots (see
/// `phera_value::Properties`): its key and slot, and the types a write may
/// store as they are.
///
/// The slot is the same in the declaring class and in every class that
/// extends it, because a layout lists the parent's properties first. So a
/// direct method checks only that the object's layout has this key there:
/// any other object (of an unrelated class, or with a lazy or native
/// table) gets the general path.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotAccess {
    pub key: crate::ArrayKey,
    pub slot: usize,
    /// `None` when a write must take the general path: a readonly
    /// property, or a type that names a class.
    pub writes: Option<phera_bytecode::TypeMask>,
}

impl SlotAccess {
    /// The property's value in `properties`, or `None` when it has none:
    /// `None` outside when the properties do not keep it in this slot.
    pub fn read<'a>(&self, properties: &'a crate::value::Properties) -> Option<Option<&'a Value>> {
        (properties.slot_for(&self.key, Some(self.slot)) == Some(self.slot))
            .then(|| properties.slot(self.slot))
    }

    /// Whether a write may store `value` as it is: a type the declaration
    /// accepts without a conversion, and not a PHP reference.
    pub fn accepts(&self, value: &Value) -> bool {
        use phera_bytecode::TypeMask;
        let Some(mask) = self.writes else {
            return false;
        };
        let own = match value {
            Value::Null => TypeMask::NULL,
            Value::Bool(true) => TypeMask::TRUE,
            Value::Bool(false) => TypeMask::FALSE,
            Value::Int(_) => TypeMask::INT,
            Value::Float(_) => TypeMask::FLOAT,
            Value::String(_) => TypeMask::STRING,
            Value::Array(_) => TypeMask::ARRAY,
            Value::Object(_) => TypeMask::OBJECT,
            _ => return false,
        };
        mask.intersects(own)
    }
}

/// A method by its place in the member list of a linked class: the class's
/// number and the method's position. The VM keeps the reflected function
/// of each in a column of that class, so that `getMethods()` finds them
/// without a name. Valid only for the current PHP request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MethodHandle {
    pub class: u32,
    pub index: u32,
}

/// A property slot in a linked class, valid only for the current PHP request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PropertyHandle {
    pub class: u32,
    pub slot: PropertySlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertySlot {
    Instance(usize),
    Static(usize),
}

#[derive(Clone, Copy, Debug)]
pub enum MemberKind {
    Method,
    Property,
    Constant,
}

/// Request-local immutable member snapshots shared by distinct reflector objects.
pub type Members = Rc<Vec<Rc<MemberInfo>>>;

/// A linked class handle avoids looking up its name again on each reflection call.
pub enum ClassSelector<'a> {
    Linked(u32),
    Name(&'a str),
}

/// Observes a generator without running any PHP instructions.
#[derive(Clone, Copy)]
pub enum GeneratorQuery {
    Line,
    File,
    This,
    Executing,
    Closed,
    Trace(i64),
}

/// Finish a Reflection reader with the initialized instance after PHP has run.
pub type LazyReflectionRead = Box<dyn FnOnce(&mut dyn crate::Context, Rc<Object>) -> NativeResult>;

/// Introspection and privileged operations requested by PHP's Reflection extension.
pub trait ReflectionContext {
    /// Reads a generator's declaration even after its activation closes.
    fn reflect_generator_function(
        &self,
        object: &Object,
    ) -> Result<FunctionTarget, crate::NativeError>;

    /// Reads a live generator's location, receiver, delegate or backtrace.
    fn reflect_generator(&mut self, object: &Rc<Object>, query: GeneratorQuery) -> NativeResult;

    /// Gets a declared global constant without emitting its read-time deprecation.
    fn reflect_constant(&self, name: &str) -> Result<ConstantInfo, crate::NativeError>;

    /// Returns a constant's PHP value without changing its metadata or emitting a diagnostic.
    fn reflect_constant_value(&self, name: &str) -> NativeResult;

    /// Gets the signature of a resolved function, method, or closure.
    fn reflect_function(
        &mut self,
        target: &FunctionTarget,
    ) -> Result<Rc<FunctionInfo>, crate::NativeError>;

    /// Invokes the exact reflected implementation using PHP's internal call rules.
    fn reflect_invoke(
        &mut self,
        target: FunctionTarget,
        receiver: Option<Rc<Object>>,
        args: PhpArray,
    ) -> LocalFuture<'_, NativeResult>;

    /// Makes a Closure from the reflected implementation and optional receiver.
    fn reflect_closure(
        &mut self,
        target: FunctionTarget,
        receiver: Option<Rc<Object>>,
    ) -> NativeResult;

    /// Reads a closure's current lexical captures.
    fn reflect_closure_uses(&self, function: &FunctionTarget) -> NativeResult;

    /// Reads static variables and closure captures for a callable.
    fn reflect_statics(&mut self, target: &FunctionTarget) -> NativeResult;

    /// Finds the method's prototype in parent classes and interfaces.
    fn reflect_prototype(&self, method: &MemberInfo) -> Option<MemberInfo>;

    /// Looks up the metadata of a shipped PHP module.
    fn reflect_extension(&self, name: &str) -> Option<ExtensionInfo>;

    /// Writes a dynamic property through the engine's regular mutation rules.
    fn reflect_dynamic_property_write(
        &mut self,
        object: Rc<Object>,
        name: &str,
        value: Value,
    ) -> NativeResult;

    /// Loads a class by name, with PHP autoloading.
    fn reflect_class(
        &mut self,
        name: &str,
    ) -> LocalFuture<'_, Result<Rc<ClassInfo>, crate::NativeError>>;

    /// Lists visible reflection members in declaration order, including non-public members.
    fn reflect_members(&mut self, class: ClassSelector<'_>, kind: MemberKind) -> Members;

    /// Reads a reflected object's stored properties, which initializes lazy objects.
    fn reflect_after_initialization(
        &mut self,
        object: Rc<Object>,
        read: LazyReflectionRead,
    ) -> NativeResult;

    /// Checks for an instance property using the engine's property-existence rules.
    fn reflect_instance_has_property(&mut self, object: Rc<Object>, name: String) -> NativeResult;

    /// Reads a constant using its declaring scope rather than the caller's scope.
    fn reflect_class_constant(&mut self, class: &str, name: &str) -> NativeResult;

    /// Creates an instance, optionally forwarding arguments to its constructor.
    fn reflect_construct(
        &mut self,
        class: &ClassInfo,
        args: Option<PhpArray>,
    ) -> LocalFuture<'_, NativeResult>;

    fn reflect_make_lazy(
        &mut self,
        class: &ClassInfo,
        request: lazy::MakeLazy,
    ) -> LocalFuture<'_, NativeResult>;

    fn reflect_lazy_object(
        &mut self,
        object: Rc<Object>,
        query: lazy::LazyClassQuery,
    ) -> NativeResult;

    fn reflect_lazy_property(
        &mut self,
        member: &MemberInfo,
        object: Rc<Object>,
        operation: lazy::LazyPropertyOperation,
    ) -> NativeResult;

    fn reflect_write_lazy_property(
        &mut self,
        member: Rc<MemberInfo>,
        object: Rc<Object>,
        value: Value,
    ) -> LocalFuture<'_, NativeResult>;

    /// Attribute construction applies the declaration's scalar typing and location.
    fn reflect_construct_attribute(
        &mut self,
        class: &ClassInfo,
        args: PhpArray,
        site: Option<AttributeSite>,
    ) -> LocalFuture<'_, NativeResult>;

    /// Evaluates one compiled declaration expression in its lexical scope.
    fn reflect_evaluate(&mut self, initializer: Initializer) -> LocalFuture<'_, NativeResult>;

    /// Finds a declared getter or setter without exposing it in the method table.
    fn reflect_property_hook(
        &self,
        member: &MemberInfo,
        kind: PropertyHookKind,
    ) -> Option<FunctionTarget>;

    fn reflect_hook_prototype(
        &self,
        property: PropertyHandle,
        kind: PropertyHookKind,
    ) -> Option<FunctionTarget>;

    fn reflect_property_settable_type(&self, member: &MemberInfo) -> Option<String>;

    /// Reads a live object property, with no caller visibility check.
    fn reflect_property_read(
        &mut self,
        member: &MemberInfo,
        object: Option<Rc<Object>>,
        mode: PropertyRead,
    ) -> NativeResult;

    /// Writes a live property, maintaining typed and readonly property rules.
    fn reflect_property_write(
        &mut self,
        member: &MemberInfo,
        object: Option<Rc<Object>>,
        value: Value,
        mode: PropertyWrite,
    ) -> NativeResult;
}

#[derive(Clone, Copy, Debug)]
pub enum PropertyWrite {
    Value,
    Raw,
    RawWithoutInitialization,
}

#[derive(Clone, Copy, Debug)]
pub enum PropertyRead {
    Raw,
    Value,
    Initialized,
    Default,
}

/// The registered surface of an extension.
#[derive(Clone)]
pub struct ExtensionInfo {
    pub name: String,
    pub version: String,
    pub classes: Vec<String>,
    pub functions: Vec<String>,
    pub constants: PhpArray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyHookKind {
    Get,
    Set,
}

impl PropertyHookKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Set => "set",
        }
    }
}

// A reflective call keeps its target even when the object passed to invoke is a subclass.
#[derive(Clone)]
pub enum FunctionTarget {
    Named(String),
    Method {
        class: String,
        name: String,
    },
    /// A method of a class's member list, by its place (`getMethods()`).
    Member(MethodHandle),
    PropertyHook {
        property: PropertyHandle,
        kind: PropertyHookKind,
    },
    Closure(Weak<Object>),
    /// The implicit __invoke method exists only on a concrete Closure object.
    ClosureInvoke(Weak<Object>),
}

#[derive(Clone)]
pub enum ParameterDefault {
    Literal(Value),
    Expression(Initializer),
    NativeEnumCase {
        class: &'static str,
        case: &'static str,
    },
}

impl From<AttributeValue> for ParameterDefault {
    fn from(value: AttributeValue) -> Self {
        match value {
            AttributeValue::Literal(value) => Self::Literal(value),
            AttributeValue::Expression(initializer) => Self::Expression(initializer),
        }
    }
}

#[derive(Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub position: usize,
    pub declared_type: Option<String>,
    pub by_reference: bool,
    pub can_pass_by_value: bool,
    pub variadic: bool,
    pub optional: bool,
    pub promoted: bool,
    pub default: Option<ParameterDefault>,
    pub default_constant: Option<String>,
    pub default_display: Option<String>,
    pub internal: bool,
    pub attributes: Vec<Rc<AttributeInfo>>,
}

#[derive(Clone)]
pub struct FunctionInfo {
    pub target: FunctionTarget,
    pub name: String,
    pub member: Option<MemberInfo>,
    pub source: Option<SourceInfo>,
    pub extension: Option<String>,
    pub parameters: Vec<ParameterInfo>,
    pub returns_reference: bool,
    pub return_type: Option<String>,
    pub tentative_return_type: Option<String>,
    pub is_closure: bool,
    pub anonymous: bool,
    pub is_static: bool,
    pub is_generator: bool,
    pub deprecated: bool,
    pub closure_this: Option<Weak<Object>>,
    pub closure_scope: Option<String>,
    pub closure_called: Option<String>,
    pub attributes: Vec<Rc<AttributeInfo>>,
}

#[derive(Clone)]
pub struct ConstantInfo {
    pub name: String,
    pub source: Option<SourceInfo>,
    pub extension: Option<String>,
    pub deprecated: bool,
    pub attributes: Vec<Rc<AttributeInfo>>,
}
