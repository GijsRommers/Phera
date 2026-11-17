//! Linked metadata shares inherited methods and defaults while each object owns its properties.

use std::{any::Any, collections::HashMap, rc::Rc};

use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::MethodEntry;
use phera_value::{ArrayKey, ClassHandle, Object, Value};

/// A linked class is immutable; its parent keeps inherited metadata alive.
pub(crate) struct Class {
    pub(crate) index: super::class_table::ClassIndex,
    pub(crate) ancestry: Vec<super::class_table::ClassIndex>,
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
    pub(crate) is_final: bool,
    pub(crate) is_readonly: bool,
    pub(crate) allows_dynamic_properties: bool,
    pub(crate) methods: super::method_table::MethodTable,
    /// Constructor presence is immutable after linking, including inheritance.
    pub(crate) constructor: Option<Rc<Method>>,
    /// Root-first property keys reproduce the layout used by native testing helpers.
    pub(crate) defaults: Vec<PropertyDefault>,
    /// Literal defaults are materialized once at linking; objects clone this COW table.
    pub(crate) property_template: Option<phera_value::PhpArray>,
    /// An absent constructor still has a shared signature for argument diagnostics.
    pub(crate) empty_constructor: crate::signatures::SignatureIndex,
    pub(crate) static_properties: Vec<PropertyDefault>,
    pub(crate) constants: HashMap<String, Rc<LinkedConstant>>,
}

/// Defaults retain their mangled keys so private ancestors do not collide.
#[derive(Clone)]
pub(crate) struct PropertyDefault {
    pub(crate) key: ArrayKey,
    pub(crate) value: Value,
    pub(crate) name: String,
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) initialized: bool,
    pub(crate) initializer: Option<crate::function_table::FunctionRef>,
    pub(crate) ty: Option<phera_bytecode::Type>,
    pub(crate) is_readonly: bool,
    pub(crate) static_initialized: Rc<std::cell::Cell<bool>>,
    pub(crate) value_initialized: Rc<std::cell::Cell<bool>>,
}

/// A method remembers its declaring scope independently of the receiver's class.
pub(crate) struct Method {
    pub(crate) declaring_index: super::class_table::ClassIndex,
    /// Keyed by this immutable linked method; populated on first call. The
    /// method owns the entry for its request lifetime, so invalidation is unnecessary.
    pub(crate) signature: crate::signatures::SignatureIndex,
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
    User(crate::function_table::FunctionRef),
    Native(MethodEntry),
}

/// Receiver, lexical scope and called class differ under inheritance and static dispatch.
#[derive(Clone)]
pub(crate) struct MethodContext {
    pub(crate) receiver: Option<Rc<Object>>,
    pub(crate) scope: super::class_table::ClassIndex,
    pub(crate) called: super::class_table::ClassIndex,
    /// A constructor's return value is discarded in favor of its allocated receiver.
    pub(crate) invocation: Invocation,
}

impl ClassHandle for Class {
    fn name(&self) -> &str {
        &self.name
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
    pub(crate) declaring: String,
    pub(crate) visibility: Visibility,
    pub(crate) is_final: bool,
    pub(crate) initializer: crate::function_table::FunctionRef,
    pub(crate) state: std::cell::RefCell<ConstantState>,
    pub(crate) ty: Option<phera_bytecode::Type>,
}

/// A failed evaluation restores Uninitialized so later accesses can retry.
pub(crate) enum ConstantState {
    Uninitialized,
    Evaluating,
    Ready(Value),
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

/// Missing constructors have no parameters; their diagnostic name is linked once.
pub(crate) fn empty_constructor(
    name: &str,
    signatures: &mut crate::signatures::Signatures,
) -> crate::signatures::SignatureIndex {
    signatures.insert(format!("{name}::__construct"), Vec::new())
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

impl Class {
    /// Scope compatibility is integer membership in immutable linked ancestry.
    pub(crate) fn includes(&self, ancestor: super::class_table::ClassIndex) -> bool {
        self.index == ancestor
            || self
                .ancestry
                .binary_search_by_key(&ancestor.0, |index| index.0)
                .is_ok()
    }
}

/// Link transitive class indexes once for method visibility and receiver compatibility.
pub(crate) fn ancestor_indexes(
    parent: &Option<Rc<Class>>,
    interfaces: &[Rc<Class>],
) -> Vec<super::class_table::ClassIndex> {
    let mut indexes = Vec::new();
    for class in parent.iter().chain(interfaces) {
        indexes.push(class.index);
        indexes.extend_from_slice(&class.ancestry);
    }
    indexes.sort_by_key(|index| index.0);
    indexes.dedup();
    indexes
}
