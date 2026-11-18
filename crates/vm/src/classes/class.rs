//! Linked metadata shares inherited methods and defaults while each object owns its properties.

use std::{any::Any, collections::HashMap, rc::Rc};

use crate::calls::call_state::ParameterSpec;
use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::MethodEntry;
use phera_value::{ArrayKey, ClassHandle, Object, Value};

/// A linked class is immutable; its parent keeps inherited metadata alive.
pub(crate) struct Class {
    /// The class's number in the request's class table.
    pub(crate) hooks: Option<std::rc::Rc<phera_extension_api::ClassHooks>>,
    /// Direct property opcodes can check their own hook in one load.
    pub(crate) property_access: Option<&'static phera_extension_api::NativePropertyAccess>,
    pub(crate) operations: super::operations::ClassOperations,
    pub(crate) index: super::ClassIndex,
    /// Only user class scopes may be assigned freely by Closure::bind.
    pub(crate) origin: ClassOrigin,
    /// The owning Rc can be recovered from an object's borrowed ClassHandle without a name lookup.
    pub(crate) identity: std::rc::Weak<Class>,
    /// Only compiled declaration names enter this cache, not arbitrary is_a inputs.
    /// Keyed by this class and resolved declaration name; immutable ancestry means
    /// results cannot change during the request, including cached negative results.
    pub(crate) declaration_matches: std::cell::RefCell<HashMap<String, bool>>,
    pub(crate) name: String,
    /// Class names are immutable PHP strings, shared by self/static/parent::class.
    pub(crate) name_value: Value,
    /// Complete lowercase ancestry is built at linking and stays immutable.
    pub(crate) ancestors: std::collections::HashSet<String>,
    pub(crate) parent: Option<Rc<Class>>,
    pub(crate) interfaces: Vec<Rc<Class>>,
    pub(crate) kind: ClassKind,
    pub(crate) is_abstract: bool,
    /// Immutable Throwable membership avoids ancestry allocation on every ordinary new.
    pub(crate) is_throwable: bool,
    pub(crate) has_native_layout: bool,
    pub(crate) is_final: bool,
    pub(crate) is_readonly: bool,
    pub(crate) allows_dynamic_properties: bool,
    pub(crate) magic: Option<Box<super::magic::MagicMethods>>,
    pub(crate) methods: HashMap<String, Rc<Method>>,
    /// The same methods by name key, built on first use by a fast call.
    pub(crate) method_table: std::cell::OnceCell<super::method_table::MethodTable>,
    /// Constructor presence is immutable after linking, including inheritance.
    pub(crate) constructor: Option<Rc<Method>>,
    /// Root-first property keys reproduce the layout used by native testing helpers.
    pub(crate) defaults: Vec<PropertyDefault>,
    /// Literal defaults are materialized once at linking; objects clone this COW table.
    pub(crate) property_template: Option<phera_value::PhpArray>,
    pub(crate) static_properties: Vec<PropertyDefault>,
    pub(crate) constants: HashMap<String, Rc<LinkedConstant>>,
    pub(crate) declaration: Option<(crate::code_table::FunctionIndex, u32)>,
}

/// Defaults retain their mangled keys so private ancestors do not collide.
#[derive(Clone)]
pub(crate) struct PropertyDefault {
    pub(crate) key: ArrayKey,
    pub(crate) value: Value,
    pub(crate) static_storage: Option<Rc<crate::reference_holders::StaticProperty>>,
    pub(crate) name: String,
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) initialized: bool,
    pub(crate) initializer: Option<crate::code_table::FunctionIndex>,
    pub(crate) ty: Option<phera_bytecode::Type>,
    pub(crate) is_readonly: bool,
    pub(crate) static_initialized: Rc<std::cell::Cell<bool>>,
    pub(crate) value_initialized: Rc<std::cell::Cell<bool>>,
}

/// A method remembers its declaring scope independently of the receiver's class.
pub(crate) struct Method {
    /// Keyed by this immutable linked method; populated on first call. The
    /// method owns the entry for its request lifetime, so invalidation is unnecessary.
    pub(crate) signature: std::cell::OnceCell<MethodSignature>,
    pub(crate) name: String,
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) is_static: bool,
    pub(crate) is_final: bool,
    pub(crate) is_abstract: bool,
    pub(crate) body: MethodBody,
}

/// Native class scopes cannot be assigned to a lexical closure.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassOrigin {
    User,
    Native,
    Closure,
}

/// Native and user methods share argument sends but execute through different handlers.
pub(crate) enum MethodBody {
    Closure(crate::closure_methods::ClosureMethod),
    User(crate::code_table::FunctionIndex),
    Native(MethodEntry),
}

/// Receiver, lexical scope and called class differ under inheritance and static dispatch.
#[derive(Clone)]
pub(crate) struct MethodContext {
    pub(crate) receiver: Option<Rc<Object>>,
    pub(crate) scope: Rc<Class>,
    pub(crate) called: Rc<Class>,
    /// A constructor's return value is discarded in favor of its allocated receiver.
    pub(crate) invocation: Invocation,
}

impl ClassHandle for Class {
    fn is_traversable(&self) -> bool {
        self.operations
            .has(super::operations::ClassOperations::TRAVERSABLE)
    }

    fn is_countable(&self) -> bool {
        self.operations
            .has(super::operations::ClassOperations::COUNTABLE)
    }

    fn is_instance_of(&self, name: &str) -> bool {
        self.is_a(name)
    }

    fn name(&self) -> &str {
        &self.name
    }
    fn has_property(&self, name: &str) -> bool {
        self.defaults.iter().any(|property| property.name == name)
    }

    fn has_native_properties(&self) -> bool {
        self.hooks
            .as_ref()
            .is_some_and(|hooks| hooks.properties.is_some())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Class {
    /// Traverse ancestry iteratively so user inheritance never grows the Rust stack.
    pub(crate) fn is_a(&self, name: &str) -> bool {
        if self.name.eq_ignore_ascii_case(name) {
            return true;
        }
        if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            self.ancestors.contains(&name.to_ascii_lowercase())
        } else {
            self.ancestors.contains(name)
        }
    }
}

/// Construction discards a method return; an ordinary call forwards it to its caller.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Invocation {
    Constructor,
    Method,
}

/// Lazy constants retain declaring scope and detect recursive initializers before borrowing values.
pub(crate) struct LinkedConstant {
    pub(crate) deprecated: Option<&'static str>,
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) is_final: bool,
    pub(crate) initializer: ConstantInitializer,
    pub(crate) ty: Option<phera_bytecode::Type>,
}

/// Native values such as `PDO::FETCH_ASSOC` never need a compiled initializer or evaluation state.
pub(crate) enum ConstantInitializer {
    Compiled {
        function: crate::code_table::FunctionIndex,
        state: std::cell::RefCell<ConstantState>,
    },
    Value(Value),
}

/// A failed evaluation restores Uninitialized so later accesses can retry.
pub(crate) enum ConstantState {
    Uninitialized,
    Evaluating,
    Ready(Value),
}

/// A literal load records its destination for the return-register check.
struct LiteralLoad {
    destination: phera_bytecode::Reg,
    value: Value,
}

/// Only literal loads followed by return can be folded without changing diagnostics.
pub(crate) fn literal_initializer(
    code: &crate::code_table::CodeTable,
    reference: &crate::code_table::FunctionIndex,
) -> Option<Value> {
    use phera_bytecode::Instruction;
    let instructions = &code.function(*reference).instructions;
    let [load, Instruction::Return { src }] = instructions.as_slice() else {
        return None;
    };
    let LiteralLoad { destination, value } = match *load {
        Instruction::LoadInt { dst, value } => LiteralLoad {
            destination: dst,
            value: Value::Int(value.into()),
        },
        Instruction::LoadBool { dst, value } => LiteralLoad {
            destination: dst,
            value: Value::Bool(value),
        },
        Instruction::LoadNull { dst } => LiteralLoad {
            destination: dst,
            value: Value::Null,
        },
        Instruction::LoadConst { dst, constant } => LiteralLoad {
            destination: dst,
            value: crate::unit_caches::value(
                code.unit(*reference).constants.get(constant.index())?,
            ),
        },
        _ => return None,
    };
    (destination == *src).then_some(value)
}

/// Templates are valid for the immutable linked class and contain no evaluated expressions.
pub(crate) fn property_template(defaults: &[PropertyDefault]) -> Option<phera_value::PhpArray> {
    let mut template = phera_value::PhpArray::new();
    for property in defaults {
        if property.initializer.is_some() {
            return None;
        }
        if property.initialized {
            template.insert(property.key.clone(), property.value.clone());
        }
    }
    Some(template)
}

/// Argument routing metadata is independent of the receiver and shared by calls.
pub(crate) struct MethodSignature {
    pub(crate) name: Rc<str>,
    pub(crate) parameters: Rc<[crate::calls::call_state::ParameterSpec]>,
}

/// Linked parents/interfaces already carry their transitive ancestry. The set is
/// keyed by lowercase type name, made before publication, and never invalidates:
/// PHP cannot change the ancestry of a linked class during its request lifetime.
pub(crate) fn ancestor_names(
    parent: &Option<Rc<Class>>,
    interfaces: &[Rc<Class>],
) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for class in parent.iter().chain(interfaces) {
        names.insert(class.name.to_ascii_lowercase());
        names.extend(class.ancestors.iter().cloned());
    }
    names
}

impl Class {
    /// Avoid repeated lowercase allocation for names from compiled type declarations.
    pub(crate) fn matches_declaration(&self, name: &str) -> bool {
        if self.name.eq_ignore_ascii_case(name) {
            return true;
        }
        if let Some(result) = self.declaration_matches.borrow().get(name) {
            return *result;
        }
        let result = self.is_a(name);
        self.declaration_matches
            .borrow_mut()
            .insert(name.to_owned(), result);
        result
    }
}

impl Method {
    /// Build receiver-independent argument metadata once for an immutable linked method.
    pub(crate) fn signature(&self, code: &crate::code_table::CodeTable) -> &MethodSignature {
        let method = self;
        method.signature.get_or_init(|| {
            let parameters: Rc<[ParameterSpec]> = match &method.body {
                MethodBody::User(reference) => code
                    .function(*reference)
                    .parameters
                    .iter()
                    .map(|parameter| ParameterSpec::user(code.unit(*reference), parameter))
                    .collect(),
                MethodBody::Closure(operation) => operation
                    .parameters()
                    .iter()
                    .map(ParameterSpec::native)
                    .collect(),
                MethodBody::Native(entry) => {
                    entry.params.iter().map(ParameterSpec::native).collect()
                }
            };
            MethodSignature {
                name: format!("{}::{}", method.declaring, method.name).into(),
                parameters,
            }
        })
    }
}

impl Class {
    /// A complete simple declaration can reuse the same membership result that
    /// matches_declaration made after resolution. Relative keywords are excluded
    /// by the caller; compound spellings never enter this name-keyed cache.
    pub(crate) fn cached_declaration_result(&self, name: &str) -> Option<bool> {
        if self.name.eq_ignore_ascii_case(name) {
            return Some(true);
        }
        self.declaration_matches.borrow().get(name).copied()
    }
}
