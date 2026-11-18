//! Cold declaration queries used by `new ReflectionClass(Foo::class)`.
//!
//! Extensions see owned snapshots and opaque initializer handles. The runtime
//! keeps ownership of linked classes, function bytecode and access checks.

use std::rc::{Rc, Weak};

use phera_value::{Object, PhpArray, Value};

use crate::{LocalFuture, NativeResult};

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
    pub name: String,
    pub kind: ReflectedClassKind,
    pub source: Option<SourceInfo>,
    pub extension: Option<String>,
    pub parent: Option<String>,
    pub interfaces: Vec<String>,
    pub traits: Vec<String>,
    pub trait_aliases: Vec<(String, String)>,
    pub modifiers: i64,
    pub is_abstract: bool,
    pub instantiable: bool,
    pub cloneable: bool,
    pub iterable: bool,
    pub anonymous: bool,
    pub constructor: Option<String>,
    pub attributes: Vec<AttributeInfo>,
}

/// A request-local handle to code that evaluates a declaration expression.
#[derive(Clone, Debug)]
pub struct Initializer {
    pub function: u32,
    pub scope: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AttributeInfo {
    pub validation_error: Option<String>,
    pub name: String,
    pub arguments: Vec<(Option<String>, AttributeValue)>,
    pub target: AttributeTarget,
}

#[derive(Clone, Debug)]
pub enum AttributeValue {
    Expression(Initializer),
    Integer(i64),
}

pub use phera_source::attributes::{AttributeTarget, builtin_flags, target_names};

#[derive(Clone, Debug, Default)]
pub struct MemberInfo {
    pub name: String,
    pub class: String,
    pub modifiers: i64,
    pub lookup_class: String,
    pub source: Option<SourceInfo>,
    pub attributes: Vec<AttributeInfo>,
    pub declared_type: Option<String>,
    pub promoted: bool,
    pub has_default: bool,
    pub dynamic: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum MemberKind {
    Method,
    Property,
    Constant,
}

/// Introspection and privileged operations requested by PHP's Reflection extension.
pub trait ReflectionContext {
    /// Gets a declared global constant without emitting its read-time deprecation.
    fn reflect_constant(&self, name: &str) -> Result<ConstantInfo, crate::NativeError>;

    /// Returns a constant's PHP value without changing its metadata or emitting a diagnostic.
    fn reflect_constant_value(&self, name: &str) -> NativeResult;

    /// Gets the signature of a resolved function, method, or closure.
    fn reflect_function(&self, target: &FunctionTarget)
    -> Result<FunctionInfo, crate::NativeError>;

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
    ) -> LocalFuture<'_, Result<ClassInfo, crate::NativeError>>;

    /// Lists visible reflection members in declaration order, including non-public members.
    fn reflect_members(&self, class: &str, kind: MemberKind) -> Vec<MemberInfo>;

    /// Reads a constant using its declaring scope rather than the caller's scope.
    fn reflect_class_constant(&mut self, class: &str, name: &str) -> NativeResult;

    /// Creates an instance, optionally forwarding arguments to its constructor.
    fn reflect_construct(
        &mut self,
        class: &str,
        args: Option<PhpArray>,
    ) -> LocalFuture<'_, NativeResult>;

    /// Evaluates one compiled declaration expression in its lexical scope.
    fn reflect_evaluate(&mut self, initializer: Initializer) -> LocalFuture<'_, NativeResult>;

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
    ) -> NativeResult;
}

#[derive(Clone, Copy, Debug)]
pub enum PropertyRead {
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

// A reflective call keeps its target even when the object passed to invoke is a subclass.
#[derive(Clone)]
pub enum FunctionTarget {
    Named(String),
    Method { class: String, name: String },
    Closure(Weak<Object>),
}

#[derive(Clone)]
pub enum ParameterDefault {
    Native(Value),
    Expression(Initializer),
}

#[derive(Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub position: usize,
    pub declared_type: Option<String>,
    pub by_reference: bool,
    pub variadic: bool,
    pub optional: bool,
    pub promoted: bool,
    pub default: Option<ParameterDefault>,
    pub default_constant: Option<String>,
    pub attributes: Vec<AttributeInfo>,
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
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Clone)]
pub struct ConstantInfo {
    pub name: String,
    pub source: Option<SourceInfo>,
    pub extension: Option<String>,
    pub deprecated: bool,
    pub attributes: Vec<AttributeInfo>,
}
