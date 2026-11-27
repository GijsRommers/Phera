//! Plain-data whole-program facts produced by closed-world AOT analysis.
//!
//! Every type here is `Send + Sync`, holds no `Rc` and no PHP values, and
//! represents only facts established by PHP's runtime semantics (never PHPDoc).
//! Each fact records the closed-world [`Assumptions`] it relies on so hybrid
//! and strict builds can safely decide which facts are admissible.

use crate::{ClassKind, Reg, TypeMask};

/// Bitset of PHP runtime value kinds proven possible for a dereferenced value.
///
/// Bits `0..=7` mirror [`TypeMask::NULL`] through [`TypeMask::OBJECT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SoundMask(pub u16);

impl SoundMask {
    pub const NONE: Self = Self(0);
    pub const NULL: Self = Self(TypeMask::NULL.0);
    pub const FALSE: Self = Self(TypeMask::FALSE.0);
    pub const TRUE: Self = Self(TypeMask::TRUE.0);
    pub const BOOL: Self = Self(TypeMask::BOOL.0);
    pub const INT: Self = Self(TypeMask::INT.0);
    pub const FLOAT: Self = Self(TypeMask::FLOAT.0);
    pub const STRING: Self = Self(TypeMask::STRING.0);
    pub const ARRAY: Self = Self(TypeMask::ARRAY.0);
    pub const OBJECT: Self = Self(TypeMask::OBJECT.0);
    pub const RESOURCE: Self = Self(1 << 8);
    /// The slot may never have been assigned yet (reading it warns and yields null).
    pub const UNDEF: Self = Self(1 << 9);
    pub const UNKNOWN: Self = Self(0x03FF);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn is_unknown(self) -> bool {
        self.0 == Self::UNKNOWN.0
    }

    /// True when the value is guaranteed to be a non-heap `Copy` scalar (never
    /// an array, string, object, resource, or undefined).
    pub const fn is_scalar_copy(self) -> bool {
        let allowed = Self::NULL.0 | Self::BOOL.0 | Self::INT.0 | Self::FLOAT.0;
        self.0 != 0 && (self.0 & !allowed) == 0
    }
}

/// Index into [`ProgramFacts::units`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct UnitRef(pub u32);

/// Index into [`ProgramFacts::classes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ClassRef(pub u32);

/// A function inside a closed-world image: `function == 0` is `{main}`, and
/// `1 + i` is `unit.functions[i]`, matching [`crate::UnitMetadata`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct FunctionRef {
    pub unit: UnitRef,
    pub function: u32,
}

/// Refinement on the `OBJECT` bit of a [`SoundType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClassBound {
    #[default]
    Any,
    /// Exact concrete class (no subclasses).
    Exact(ClassRef),
    /// This class or any closed-world subclass/implementor.
    Within(ClassRef),
}

/// Strictly runtime-enforced type of a dereferenced PHP value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundType {
    pub mask: SoundMask,
    pub class: ClassBound,
}

impl Default for SoundType {
    fn default() -> Self {
        Self::UNKNOWN
    }
}

impl SoundType {
    pub const NEVER: Self = Self {
        mask: SoundMask::NONE,
        class: ClassBound::Any,
    };
    pub const NULL: Self = Self {
        mask: SoundMask::NULL,
        class: ClassBound::Any,
    };
    pub const BOOL: Self = Self {
        mask: SoundMask::BOOL,
        class: ClassBound::Any,
    };
    pub const INT: Self = Self {
        mask: SoundMask::INT,
        class: ClassBound::Any,
    };
    pub const FLOAT: Self = Self {
        mask: SoundMask::FLOAT,
        class: ClassBound::Any,
    };
    pub const STRING: Self = Self {
        mask: SoundMask::STRING,
        class: ClassBound::Any,
    };
    pub const ARRAY: Self = Self {
        mask: SoundMask::ARRAY,
        class: ClassBound::Any,
    };
    pub const UNKNOWN: Self = Self {
        mask: SoundMask::UNKNOWN,
        class: ClassBound::Any,
    };

    pub const fn exact_class(class: ClassRef) -> Self {
        Self {
            mask: SoundMask::OBJECT,
            class: ClassBound::Exact(class),
        }
    }

    pub const fn within_class(class: ClassRef) -> Self {
        Self {
            mask: SoundMask::OBJECT,
            class: ClassBound::Within(class),
        }
    }

    pub const fn is_unknown(self) -> bool {
        self.mask.is_unknown() && matches!(self.class, ClassBound::Any)
    }

    pub fn union(self, other: Self) -> Self {
        let mask = self.mask.union(other.mask);
        let class = match (
            self.mask.intersects(SoundMask::OBJECT),
            other.mask.intersects(SoundMask::OBJECT),
        ) {
            (false, false) => ClassBound::Any,
            (true, false) => self.class,
            (false, true) => other.class,
            (true, true) => match (self.class, other.class) {
                (a, b) if a == b => a,
                (ClassBound::Exact(a), ClassBound::Within(b))
                | (ClassBound::Within(b), ClassBound::Exact(a))
                    if a == b =>
                {
                    ClassBound::Within(b)
                }
                _ => ClassBound::Any,
            },
        };
        Self { mask, class }
    }
}

/// Aliasing and initialization facts for one named local variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LocalFlags(pub u8);

impl LocalFlags {
    pub const NONE: Self = Self(0);
    /// Variable may hold or be bound to a PHP reference.
    pub const MAYBE_REF: Self = Self(1 << 0);
    /// Variable may be read or written dynamically by name (`$$x`, `extract`, etc.).
    pub const BY_NAME: Self = Self(1 << 1);
    /// Variable may be read before being assigned.
    pub const MAYBE_UNDEF: Self = Self(1 << 2);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn is_proven_non_ref(self) -> bool {
        !self.contains(Self::MAYBE_REF) && !self.contains(Self::BY_NAME)
    }
}

/// Closed-world assumptions required for a fact to remain sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Assumptions(pub u8);

impl Assumptions {
    pub const NONE: Self = Self(0);
    /// No undeclared functions, classes, or aliases can be introduced at runtime.
    pub const SEALED_NAMES: Self = Self(1 << 0);
    /// No new subclasses or implementations can be declared at runtime.
    pub const SEALED_HIERARCHY: Self = Self(1 << 1);
    /// The target unit is guaranteed to have been loaded before execution reaches the site.
    pub const UNIT_LOADED: Self = Self(1 << 2);

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

/// Build strictness controlling runtime fallback and closed-world sealing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BuildMode {
    /// Dynamic includes outside the image fall back to disk compilation; only
    /// assumption-free facts (or verified sealed invariants) apply.
    #[default]
    Hybrid,
    /// Closed-world production image: unknown includes, stream includes, and
    /// unallowed `eval()` fail at runtime.
    Strict,
}

/// Declaration multiplicity across the discovered closed world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DeclUniqueness {
    /// Exactly one declaration exists in the discovered world.
    Unique { unit: UnitRef, hoisted: bool },
    /// Multiple declarations share the same lowercased name.
    Duplicate,
    /// Provided by the runtime extension registry.
    Native,
    /// Not found in the discovered closed world.
    #[default]
    Unknown,
}

/// Property flags recorded on an [`AotPropSlot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PropFlags(pub u8);

impl PropFlags {
    pub const NONE: Self = Self(0);
    pub const READONLY: Self = Self(1 << 0);
    pub const STATIC: Self = Self(1 << 1);
    pub const HAS_HOOKS: Self = Self(1 << 2);
    pub const TYPED: Self = Self(1 << 3);
    pub const PRIVATE: Self = Self(1 << 4);
    pub const PROTECTED: Self = Self(1 << 5);
    pub const PUBLIC: Self = Self(1 << 6);
    pub const SET_RESTRICTED: Self = Self(1 << 7);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Method flags recorded on an [`AotMethod`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MethodFlags(pub u8);

impl MethodFlags {
    pub const NONE: Self = Self(0);
    pub const STATIC: Self = Self(1 << 0);
    pub const FINAL: Self = Self(1 << 1);
    pub const ABSTRACT: Self = Self(1 << 2);
    pub const PRIVATE: Self = Self(1 << 3);
    pub const PROTECTED: Self = Self(1 << 4);
    pub const PUBLIC: Self = Self(1 << 5);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Class-level closed-world facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ClassFlags(pub u16);

impl ClassFlags {
    pub const NONE: Self = Self(0);
    pub const FINAL: Self = Self(1 << 0);
    pub const EFFECTIVELY_FINAL: Self = Self(1 << 1);
    pub const ABSTRACT: Self = Self(1 << 2);
    pub const READONLY: Self = Self(1 << 3);
    pub const LAYOUT_EXACT: Self = Self(1 << 4);
    pub const DYNAMIC_PROPS: Self = Self(1 << 5);
    pub const MAGIC_GET: Self = Self(1 << 6);
    pub const MAGIC_SET: Self = Self(1 << 7);
    pub const MAGIC_CALL: Self = Self(1 << 8);
    pub const MAGIC_CALL_STATIC: Self = Self(1 << 9);
    pub const MAGIC_DESTRUCT: Self = Self(1 << 10);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Call-site specialization flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CallFlags(pub u8);

impl CallFlags {
    pub const NONE: Self = Self(0);
    /// Every argument's sound type is an exact non-coercing subtype of the
    /// callee's declared parameter type.
    pub const ARGS_EXACT: Self = Self(1 << 0);
    /// The callee's return value is proven to satisfy its declared return type.
    pub const RETURN_PROVEN: Self = Self(1 << 1);
    /// Target is a pure native function.
    pub const PURE_NATIVE: Self = Self(1 << 2);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Property access flags at a resolved site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PropAccess(pub u8);

impl PropAccess {
    pub const NONE: Self = Self(0);
    pub const READ_EXACT: Self = Self(1 << 0);
    pub const WRITE_EXACT: Self = Self(1 << 1);
    pub const SKIP_TYPE_CHECK: Self = Self(1 << 2);
    pub const NON_READONLY: Self = Self(1 << 3);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Function-level closed-world summary flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FunctionFlags(pub u8);

impl FunctionFlags {
    pub const NONE: Self = Self(0);
    pub const DYNAMIC_SCOPE: Self = Self(1 << 0);
    pub const IS_GENERATOR: Self = Self(1 << 1);
    pub const FAST_CANDIDATE: Self = Self(1 << 2);
    pub const RETURN_PROVEN: Self = Self(1 << 3);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// One declared instance property slot in root-first `PropertyLayout` order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotPropSlot {
    /// Mangled PHP storage key (`name`, `\0*\0name`, or `\0Class\0name`).
    pub key: Box<[u8]>,
    pub declaring: ClassRef,
    pub flags: PropFlags,
    pub ty: SoundType,
}

/// One method entry in an [`AotClassLayout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotMethod {
    /// Lowercased method name.
    pub name: Box<str>,
    pub function: FunctionRef,
    pub flags: MethodFlags,
}

/// Whole-program summary for one class, interface, trait, or enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotClassLayout {
    pub name: Box<str>,
    pub decl: DeclUniqueness,
    pub kind: ClassKind,
    pub parent: Option<ClassRef>,
    pub ancestors: Box<[ClassRef]>,
    /// Start index in [`ProgramFacts::descendants`].
    pub first_descendant: u32,
    /// Number of descendants in [`ProgramFacts::descendants`].
    pub descendants: u32,
    pub flags: ClassFlags,
    /// Root-first instance property slots (populated when `LAYOUT_EXACT` holds).
    pub slots: Box<[AotPropSlot]>,
    /// Methods sorted by lowercased name.
    pub methods: Box<[AotMethod]>,
}

/// Statically resolved call target at instruction index `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedCall {
    pub at: u32,
    pub target: FunctionRef,
    pub needs: Assumptions,
    pub flags: CallFlags,
    pub result: SoundType,
}

/// Statically resolved property slot at instruction index `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedProp {
    pub at: u32,
    pub class: ClassRef,
    pub slot: u16,
    pub needs: Assumptions,
    pub flags: PropAccess,
    pub ty: SoundType,
}

/// Per-function whole-program facts stored alongside a [`crate::CompiledUnit`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionBuildFacts {
    pub flags: FunctionFlags,
    pub locals: Box<[LocalFlags]>,
    /// Instruction indices where basic blocks begin (sorted ascending).
    pub block_starts: Box<[u32]>,
    /// Non-`UNKNOWN` register types at block entry: `(block_index, reg, ty)`, sorted.
    pub block_types: Box<[(u16, Reg, SoundType)]>,
    /// Resolved calls sorted by instruction index `at`.
    pub calls: Box<[ResolvedCall]>,
    /// Resolved property accesses sorted by instruction index `at`.
    pub props: Box<[ResolvedProp]>,
    pub returns: SoundType,
}

/// Whole-program facts attached to one [`crate::CompiledUnit`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitBuildFacts {
    pub unit: UnitRef,
    pub classes: Box<[ClassRef]>,
    /// Index 0 is `{main}`, `1 + i` is `unit.functions[i]`.
    pub functions: Box<[FunctionBuildFacts]>,
}

/// Dynamic PHP feature detected during closed-world auditing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicKind {
    Eval,
    DynamicInclude,
    DynamicNew,
    DynamicStaticCall,
    DynamicMethodCall,
    ClassAlias,
    Extract,
    Compact,
    Unserialize,
    Reflection,
    VariableVariable,
}

impl DynamicKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eval => "eval",
            Self::DynamicInclude => "dynamic_include",
            Self::DynamicNew => "dynamic_new",
            Self::DynamicStaticCall => "dynamic_static_call",
            Self::DynamicMethodCall => "dynamic_method_call",
            Self::ClassAlias => "class_alias",
            Self::Extract => "extract",
            Self::Compact => "compact",
            Self::Unserialize => "unserialize",
            Self::Reflection => "reflection",
            Self::VariableVariable => "variable_variable",
        }
    }

    pub fn parse_allow(name: &str) -> Option<Self> {
        match name {
            "eval" => Some(Self::Eval),
            "dynamic_include" => Some(Self::DynamicInclude),
            "dynamic_new" => Some(Self::DynamicNew),
            "dynamic_static_call" => Some(Self::DynamicStaticCall),
            "dynamic_method_call" => Some(Self::DynamicMethodCall),
            "class_alias" => Some(Self::ClassAlias),
            "extract" => Some(Self::Extract),
            "compact" => Some(Self::Compact),
            "unserialize" => Some(Self::Unserialize),
            "reflection" => Some(Self::Reflection),
            "variable_variable" => Some(Self::VariableVariable),
            _ => None,
        }
    }
}

/// Scope of invariants invalidated by a [`DynamicFinding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlastRadius {
    /// Affects only the enclosing function (local scope).
    Function,
    /// Affects a single class or dynamic object instantiation.
    Class,
    /// Can introduce arbitrary symbols or aliases across the whole program.
    Names,
}

impl BlastRadius {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Names => "names",
        }
    }
}

/// One dynamic feature occurrence recorded during closed-world auditing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicFinding {
    pub kind: DynamicKind,
    pub radius: BlastRadius,
    pub at: FunctionRef,
    pub line: u32,
    pub detail: Box<str>,
}

/// Complete audit report of dynamic features found across the closed world.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DynamicAudit {
    pub findings: Box<[DynamicFinding]>,
    /// Closed-world assumptions that hold given the findings and build mode.
    pub holds: Assumptions,
}

/// Whole-program fact table serialized into `.phb` images and shared across workers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramFacts {
    pub mode: BuildMode,
    /// Unit paths indexed by [`UnitRef`].
    pub units: Box<[Box<str>]>,
    /// Class layouts sorted by lowercased name, indexed by [`ClassRef`].
    pub classes: Box<[AotClassLayout]>,
    /// Flattened descendant slices referenced by [`AotClassLayout::first_descendant`].
    pub descendants: Box<[ClassRef]>,
    /// Functions sorted by lowercased name.
    pub functions: Box<[(Box<str>, DeclUniqueness, FunctionRef)]>,
    pub audit: DynamicAudit,
}

impl ProgramFacts {
    pub fn descendants_of(&self, class: ClassRef) -> &[ClassRef] {
        let Some(layout) = self.classes.get(class.0 as usize) else {
            return &[];
        };
        let start = layout.first_descendant as usize;
        let end = start.saturating_add(layout.descendants as usize);
        self.descendants.get(start..end).unwrap_or(&[])
    }
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ProgramFacts>();
    assert_send_sync::<UnitBuildFacts>();
    assert_send_sync::<FunctionBuildFacts>();
    assert_send_sync::<AotClassLayout>();
    assert_send_sync::<DynamicAudit>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_type_union_preserves_class_bounds_when_compatible() {
        let a = ClassRef(1);
        let b = ClassRef(2);
        assert_eq!(
            SoundType::exact_class(a).union(SoundType::NULL),
            SoundType {
                mask: SoundMask::OBJECT.union(SoundMask::NULL),
                class: ClassBound::Exact(a),
            }
        );
        assert_eq!(
            SoundType::exact_class(a).union(SoundType::within_class(a)),
            SoundType::within_class(a)
        );
        assert_eq!(
            SoundType::exact_class(a).union(SoundType::exact_class(b)),
            SoundType {
                mask: SoundMask::OBJECT,
                class: ClassBound::Any,
            }
        );
    }

    #[test]
    fn sound_mask_scalar_copy_distinguishes_heap_and_undef() {
        assert!(SoundMask::INT.is_scalar_copy());
        assert!(SoundMask::INT.union(SoundMask::FLOAT).is_scalar_copy());
        assert!(SoundMask::NULL.union(SoundMask::BOOL).is_scalar_copy());
        assert!(!SoundMask::NONE.is_scalar_copy());
        assert!(!SoundMask::STRING.is_scalar_copy());
        assert!(!SoundMask::INT.union(SoundMask::UNDEF).is_scalar_copy());
        assert!(!SoundMask::UNKNOWN.is_scalar_copy());
    }
}
