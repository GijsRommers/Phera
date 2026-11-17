//! Permanent declaration links replace guarded callable caches.
//!
//! After `function f() {}` is declared, every `f()` links to the same numeric
//! body and signature. New declarations fill empty entries; old links stay valid.

use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::signatures::Signatures;
use phera_bytecode::CompiledUnit;
use std::sync::Arc;

/// VM services are registered as permanent declarations alongside extension functions.
const INTRINSIC_NAMES: &[&str] = &[
    "set_exception_handler",
    "restore_exception_handler",
    "set_error_handler",
    "restore_error_handler",
    "trigger_error",
    "user_error",
    "array_map",
    "basename",
    "get_object_vars",
    "get_class_vars",
    "property_exists",
    "is_callable",
    "call_user_func",
    "call_user_func_array",
    "get_defined_vars",
    "compact",
    "extract",
    "define",
    "defined",
    "constant",
    "function_exists",
    "func_num_args",
    "func_get_args",
    "func_get_arg",
    "error_reporting",
    "ini_set",
    "ini_get",
    "get_class",
    "get_parent_class",
    "get_called_class",
    "method_exists",
    "class_exists",
    "interface_exists",
    "is_a",
    "is_subclass_of",
    "spl_object_id",
    "spl_object_hash",
    "get_class_methods",
    "class_parents",
    "class_implements",
    "get_declared_classes",
];

/// A declaration retains source metadata for cold errors and initializers.
#[derive(Clone)]
pub(crate) struct FunctionRef {
    pub(crate) unit: Arc<CompiledUnit>,
    pub(crate) index: usize,
    pub(crate) address: crate::code_table::FunctionIndex,
}

/// All callable names address flat declaration records, including native functions.
pub(crate) struct FunctionTable {
    names: crate::names::Names,
    declarations: Vec<Option<FunctionRef>>,
    callees: Vec<Option<Callee>>,
}

impl FunctionTable {
    /// Native functions and VM services are declarations available before any unit is loaded.
    pub(crate) fn new(
        registry: &phera_extension_api::Registry,
        signatures: &mut Signatures,
    ) -> Self {
        let mut table = Self {
            names: Default::default(),
            declarations: Vec::new(),
            callees: Vec::new(),
        };
        for entry in registry.functions() {
            let signature = signatures.insert(
                entry.name.into(),
                entry.params.iter().map(ParameterSpec::native).collect(),
            );
            table.publish(
                entry.name,
                Callee {
                    signature,
                    argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
                    target: CallTarget::Native(signatures.insert_native(*entry)),
                },
            );
        }
        for &name in INTRINSIC_NAMES {
            if table.callee(name).is_some() {
                continue;
            }
            let Some(intrinsic) = crate::intrinsics::Intrinsic::resolve(name) else {
                continue;
            };
            let parameters = intrinsic
                .parameter_names()
                .iter()
                .map(|name| ParameterSpec {
                    name: (*name).into(),
                    ty: None,
                    by_reference: matches!(intrinsic, crate::intrinsics::Intrinsic::IsCallable)
                        && *name == "callable_name",
                    variadic: matches!(intrinsic, crate::intrinsics::Intrinsic::CallUserFunc)
                        && *name == "args",
                })
                .collect();
            let signature = signatures.insert(name.into(), parameters);
            table.publish(
                name,
                Callee {
                    signature,
                    argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
                    target: CallTarget::Intrinsic(intrinsic),
                },
            );
        }
        table
    }

    /// Publication changes no previously linked call site.
    fn publish(&mut self, name: &str, callee: Callee) -> usize {
        let index = self.names.intern(name).0 as usize;
        self.callees
            .resize_with(self.callees.len().max(index + 1), || None);
        self.declarations.resize_with(self.callees.len(), || None);
        self.callees[index] = Some(callee);
        index
    }

    /// Runtime strings intern at the callable boundary; loaded symbols resolve only once.
    pub(crate) fn callee(&self, name: &str) -> Option<Callee> {
        self.callees.get(self.names.find(name)?.0 as usize)?.clone()
    }

    /// Cold redeclaration diagnostics need the original source location.
    pub(crate) fn get(&self, name: &str) -> Option<&FunctionRef> {
        self.declarations
            .get(self.names.find(name)?.0 as usize)?
            .as_ref()
    }

    /// PHP function_exists includes native and intrinsic declarations.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.callee(name).is_some()
    }

    /// A PHP declaration publishes the body's already linked signature and numeric address.
    pub(crate) fn insert(
        &mut self,
        name: String,
        function: FunctionRef,
        code: &crate::code_table::CodeTable,
    ) {
        let index = self.publish(
            &name,
            Callee {
                argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
                signature: code.functions[function.address.0 as usize].signature,
                target: CallTarget::User {
                    function: function.address,
                },
            },
        );
        self.declarations[index] = Some(function);
    }
}
