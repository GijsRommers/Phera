//! `ReflectionFunction('strlen')->getParameters()` reads registered native signatures.

use phera_extension_api::reflection::{
    AttributeTarget, FunctionInfo, FunctionTarget, Initializer, MemberInfo, ParameterDefault,
    ParameterInfo,
};
use phera_extension_api::{MethodReturn, NativeError, Param};
use std::rc::Rc;

use crate::Vm;
use crate::calls::call_state::CallTarget;
use crate::classes::{Method, MethodBody};
use crate::closures::{ClosureOrigin, ClosureState};
use crate::code_table::FunctionIndex;

use super::{exception, flags, source_info, visibility_bits};

impl Vm {
    pub(super) fn reflection_function(
        &self,
        target: &FunctionTarget,
    ) -> Result<FunctionInfo, NativeError> {
        let mut reflected = FunctionInfo {
            target: target.clone(),
            name: String::new(),
            member: None,
            source: None,
            extension: None,
            parameters: Vec::new(),
            returns_reference: false,
            return_type: None,
            tentative_return_type: None,
            is_closure: false,
            anonymous: false,
            is_static: false,
            is_generator: false,
            deprecated: false,
            closure_this: None,
            closure_scope: None,
            closure_called: None,
            attributes: Vec::new(),
        };
        match target {
            FunctionTarget::Named(name) => {
                let callee = self
                    .resolve_callee(name)
                    .map_err(|_| exception(format!("Function {name}() does not exist")))?;
                reflected.name = callee.name.to_string();
                self.reflect_callee(&mut reflected, &callee.target);
            }
            FunctionTarget::Method { class, name } => {
                let (class, method) = self.reflection_method(class, name)?;
                self.reflect_method_signature(&mut reflected, &method, &class.name);
                reflected.member = Some(MemberInfo {
                    name: method.name.clone(),
                    class: method.declaring.clone(),
                    lookup_class: class.name.clone(),
                    modifiers: visibility_bits(method.visibility)
                        | flags(method.is_static, 16)
                        | flags(method.is_final, 32)
                        | flags(method.is_abstract, 64),
                    ..MemberInfo::default()
                });
            }
            FunctionTarget::Closure(object) => {
                let object = object
                    .upgrade()
                    .ok_or_else(|| exception("Closure is unavailable"))?;
                let storage = object
                    .native
                    .try_borrow()
                    .map_err(|_| exception("Closure is in use"))?;
                let closure = storage
                    .as_ref()
                    .and_then(|state| state.downcast_ref::<ClosureState>())
                    .ok_or_else(|| exception("Closure is uninitialized"))?;
                reflected.name = closure.callee.name.to_string();
                self.reflect_callee(&mut reflected, &closure.callee.target);
                reflected.is_closure = true;
                reflected.anonymous = closure.origin == ClosureOrigin::Lexical;
                reflected.is_static = closure.binding == phera_bytecode::ClosureBinding::Static;
                if let Some(context) = &closure.context {
                    reflected.closure_this = context.receiver.as_ref().map(Rc::downgrade);
                    reflected.closure_scope = Some(context.scope.name.clone());
                    reflected.closure_called = Some(context.called.name.clone());
                }
            }
        }
        Ok(reflected)
    }

    fn reflect_callee(&self, reflected: &mut FunctionInfo, target: &CallTarget) {
        match target {
            CallTarget::User(function) => self.reflect_user_signature(reflected, *function, None),
            CallTarget::Native(entry) => {
                reflected.parameters = native_parameters(entry.params);
                reflected.extension = self.registry.function_module(entry.name).map(str::to_owned);
            }
            CallTarget::Method { method, context } => {
                self.reflect_method_signature(reflected, method, &context.called.name);
            }
            CallTarget::Intrinsic(entry) => {
                reflected.parameters = entry.parameters().map_or_else(
                    || {
                        entry
                            .parameter_names()
                            .iter()
                            .enumerate()
                            .map(|(position, name)| {
                                native_parameter(
                                    position,
                                    Param::new(name, phera_extension_api::ParamType::Mixed),
                                )
                            })
                            .collect()
                    },
                    native_parameters,
                );
                reflected.extension = Some("Core".into());
            }
            CallTarget::Closure(_) => {}
        }
    }

    fn reflect_method_signature(
        &self,
        reflected: &mut FunctionInfo,
        method: &Method,
        _called: &str,
    ) {
        reflected.name = method.name.clone();
        reflected.is_static = method.is_static;
        match method.body {
            MethodBody::User(function) => {
                self.reflect_user_signature(reflected, function, Some(&method.declaring));
            }
            MethodBody::Native(entry) => {
                reflected.parameters = native_parameters(entry.params);
                reflected.extension = self
                    .registry
                    .class_module(&method.declaring)
                    .map(str::to_owned);
                match entry.return_type {
                    Some(MethodReturn::Required(ty)) => reflected.return_type = Some(ty.into()),
                    Some(MethodReturn::Tentative(ty)) => {
                        reflected.tentative_return_type = Some(ty.into())
                    }
                    None => {}
                }
            }
            MethodBody::Closure(operation) => {
                reflected.parameters = native_parameters(operation.parameters());
                reflected.extension = Some("Core".into());
            }
        }
    }

    fn reflect_user_signature(
        &self,
        reflected: &mut FunctionInfo,
        function: FunctionIndex,
        scope: Option<&str>,
    ) {
        let unit = self.code.unit(function);
        let body = self.code.function(function);
        let first = self.code.facts(function).unit_first;
        let metadata = self
            .code
            .local(function)
            .and_then(|index| unit.function_metadata.get(index));
        reflected.source = Some(metadata.map_or_else(
            || phera_extension_api::reflection::SourceInfo {
                file: unit.file.clone(),
                start_line: body.definition_line,
                end_line: body.definition_line,
                doc_comment: None,
            },
            |metadata| source_info(&unit.file, &metadata.declaration),
        ));
        if let Some(metadata) = metadata {
            let target = if scope.is_some() {
                AttributeTarget::Method
            } else {
                AttributeTarget::Function
            };
            reflected.attributes = self.reflection_attributes(
                function,
                &metadata.declaration.attributes,
                target,
                scope,
            );
        }
        reflected.deprecated = reflected
            .attributes
            .iter()
            .any(|attr| attr.name.eq_ignore_ascii_case("Deprecated"));
        reflected.return_type = body
            .return_type
            .map(|id| unit.types[id.index()].display.clone());
        reflected.returns_reference = body.returns_reference;
        let required = body
            .parameters
            .iter()
            .rposition(|param| !param.has_default && !param.variadic);
        reflected.parameters = body
            .parameters
            .iter()
            .enumerate()
            .map(|(position, param)| {
                let metadata = metadata.and_then(|metadata| metadata.parameters.get(position));
                let optional = required.is_none_or(|required| position > required);
                let declared_type = param.ty.map(|id| {
                    let ty = &unit.types[id.index()].display;
                    if param.default_display.as_deref() == Some("null")
                        && !ty.starts_with('?')
                        && !ty.contains('|')
                        && ty != "mixed"
                        && ty != "null"
                    {
                        format!("?{ty}")
                    } else {
                        ty.clone()
                    }
                });
                ParameterInfo {
                    name: unit.names[param.name.index()].clone(),
                    position,
                    declared_type,
                    by_reference: param.by_reference,
                    variadic: param.variadic,
                    optional,
                    promoted: metadata.is_some_and(|meta| meta.promoted),
                    default: metadata
                        .and_then(|meta| meta.default)
                        .filter(|_| optional)
                        .map(|default| {
                            ParameterDefault::Expression(Initializer {
                                function: self.code.declared(first, default.index()).0,
                                scope: scope.map(str::to_owned),
                            })
                        }),
                    default_constant: metadata
                        .filter(|_| optional)
                        .and_then(|meta| meta.default_constant.clone()),
                    attributes: metadata.map_or_else(Vec::new, |meta| {
                        self.reflection_attributes(
                            function,
                            &meta.attributes,
                            AttributeTarget::Parameter,
                            scope,
                        )
                    }),
                }
            })
            .collect();
    }
}

fn native_parameters(parameters: &[Param]) -> Vec<ParameterInfo> {
    parameters
        .iter()
        .copied()
        .enumerate()
        .map(|(position, param)| native_parameter(position, param))
        .collect()
}

fn native_parameter(position: usize, param: Param) -> ParameterInfo {
    let name = param.ty.name();
    let declared_type = if param.nullable && name != "mixed" {
        if name.contains('|') {
            format!("{name}|null")
        } else {
            format!("?{name}")
        }
    } else {
        name.into()
    };
    ParameterInfo {
        name: param.name.into(),
        position,
        declared_type: Some(declared_type),
        by_reference: param.by_ref,
        variadic: param.variadic,
        optional: param.optional,
        promoted: false,
        default: param
            .default
            .map(|value| ParameterDefault::Native(value.to_value())),
        default_constant: None,
        attributes: Vec::new(),
    }
}
