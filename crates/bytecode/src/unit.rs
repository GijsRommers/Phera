//! A compiled file and the tables shared by its functions.

use crate::{Function, FunctionId, NameId};

/// One compiled PHP file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompiledUnit {
    /// The path the file was compiled from, as `__FILE__` reports it.
    pub file: String,
    /// Non-fatal declaration diagnostics are emitted when this file is compiled for execution.
    pub declaration_warnings: Vec<DeclarationWarning>,
    /// The file's top-level code.
    pub main: Function,
    /// Every function the file declares, in source order.
    pub functions: Vec<Function>,
    /// Source class declarations; runtime linking is request-local.
    pub classes: Vec<crate::ClassDeclaration>,
    /// Unconditional declarations eligible for dependency-aware prelinking.
    pub hoisted_classes: Vec<crate::ClassId>,
    /// The functions declared unconditionally at the top level. PHP makes
    /// these callable before the file starts running.
    pub hoisted: Vec<FunctionId>,
    pub constants: Vec<Constant>,
    /// Variable names, parameter names and other identifiers, each stored
    /// once.
    pub names: Vec<String>,
    /// Functions and constants the code refers to by name.
    pub symbols: Vec<Symbol>,
    pub types: Vec<Type>,
}

/// A literal value. Converted to a PHP value by the VM when loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// PHP strings are bytes, not necessarily UTF-8.
    String(Vec<u8>),
    /// An array literal made only of literals, in source order.
    Array(Vec<(ConstantKey, Constant)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstantKey {
    /// No key was written: the next free integer.
    Next,
    Int(i64),
    String(Vec<u8>),
}

/// A function or constant mentioned by name, as the compiler resolved it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Symbol {
    /// The fully qualified name, without a leading backslash. Function names
    /// are lowercased, because PHP ignores their case; constant names keep
    /// theirs except for the namespace part.
    pub name: String,
    /// An unqualified name inside a namespace (`strlen` in `namespace App`)
    /// means `App\strlen` if that exists and the global `strlen` otherwise.
    /// This is the global name to try second.
    pub fallback: Option<String>,
    /// The name as the source wrote it, for error messages.
    pub written: String,
}

/// A declared type: `int`, `?string`, `int|float`, `Foo&Bar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    /// The built-in types the declaration allows.
    pub mask: TypeMask,
    /// The class names it allows, lowercased and fully qualified.
    pub classes: Vec<NameId>,
    /// Whether `classes` must all match (`A&B`) rather than any (`A|B`).
    pub intersection: bool,
    /// The type as PHP prints it in a `TypeError`: `?int`, `string|int`.
    pub display: String,
}

/// A set of built-in types, one bit each, so a type check is a bit test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TypeMask(pub u16);

impl TypeMask {
    pub const NULL: TypeMask = TypeMask(1);
    pub const FALSE: TypeMask = TypeMask(1 << 1);
    pub const TRUE: TypeMask = TypeMask(1 << 2);
    pub const BOOL: TypeMask = TypeMask(Self::FALSE.0 | Self::TRUE.0);
    pub const INT: TypeMask = TypeMask(1 << 3);
    pub const FLOAT: TypeMask = TypeMask(1 << 4);
    pub const STRING: TypeMask = TypeMask(1 << 5);
    pub const ARRAY: TypeMask = TypeMask(1 << 6);
    pub const OBJECT: TypeMask = TypeMask(1 << 7);
    pub const CALLABLE: TypeMask = TypeMask(1 << 8);
    pub const ITERABLE: TypeMask = TypeMask(1 << 9);
    /// Only as a return type: the function returns nothing.
    pub const VOID: TypeMask = TypeMask(1 << 10);
    /// Only as a return type: the function never returns.
    pub const NEVER: TypeMask = TypeMask(1 << 11);
    /// Only as a return type: an instance of the called class.
    pub const STATIC: TypeMask = TypeMask(1 << 12);
    pub const MIXED: TypeMask = TypeMask(
        Self::NULL.0
            | Self::BOOL.0
            | Self::INT.0
            | Self::FLOAT.0
            | Self::STRING.0
            | Self::ARRAY.0
            | Self::OBJECT.0,
    );

    pub const fn contains(self, other: TypeMask) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: TypeMask) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: TypeMask) -> TypeMask {
        TypeMask(self.0 | other.0)
    }
}

// A unit is shared between worker threads through an `Arc`.
const _: fn() = || {
    fn shared_between_threads<T: Send + Sync>() {}
    shared_between_threads::<CompiledUnit>();
};

/// PHP reports these before executing file code, even for declarations in an untaken branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationWarning {
    pub message: String,
    pub line: u32,
}
