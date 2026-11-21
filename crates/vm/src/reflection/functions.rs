//! `ReflectionFunction('strlen')->getParameters()` reads registered native signatures.

use phera_extension_api::reflection::{
    AttributeTarget, FunctionInfo, FunctionTarget, MemberInfo, ParameterDefault, ParameterInfo,
};
use phera_extension_api::{
    MethodReturn, NativeError, Param,
    native_parameters::{StubDefault, StubParameter, signature},
};
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
            // `Vm::member_function` turns it into a method by name first.
            FunctionTarget::Member(_) => return Err(internal_error!("reflected member target")),
            FunctionTarget::Named(name) => {
                let callee = self
                    .resolve_callee(name)
                    .map_err(|_| exception(format!("Function {name}() does not exist")))?;
                reflected.name = callee.name.to_string();
                self.reflect_callee(&mut reflected, &callee.target, None);
            }
            FunctionTarget::Method { .. }
            | FunctionTarget::LinkedMethod { .. }
            | FunctionTarget::PropertyHook { .. } => {
                let (class, method) = self.reflection_target_method(target)?;
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
            FunctionTarget::Closure(object) | FunctionTarget::ClosureInvoke(object) => {
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
                self.reflect_callee(
                    &mut reflected,
                    &closure.callee.target,
                    closure
                        .context
                        .as_ref()
                        .map(|context| context.scope.name.as_str()),
                );
                if matches!(target, FunctionTarget::ClosureInvoke(_)) {
                    reflected.name = "__invoke".into();
                    reflected.member = Some(MemberInfo {
                        name: reflected.name.clone(),
                        class: "Closure".into(),
                        lookup_class: "Closure".into(),
                        modifiers: 1,
                        ..MemberInfo::default()
                    });
                    reflected.source = None;
                    reflected.extension = None;
                    reflected.is_static = false;
                    reflected.attributes.clear();
                    for parameter in &mut reflected.parameters {
                        parameter.default = None;
                        parameter.default_constant = None;
                        parameter.default_display = None;
                    }
                    return Ok(reflected);
                }
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

    fn reflect_callee(
        &self,
        reflected: &mut FunctionInfo,
        target: &CallTarget,
        scope: Option<&str>,
    ) {
        match target {
            CallTarget::User(function) => {
                self.reflect_user_signature(reflected, *function, scope, AttributeTarget::Function);
            }
            CallTarget::Native(entry) => {
                use phera_source::deprecations::{ItemKind, kept_notice};
                reflected.deprecated = kept_notice(ItemKind::Function, entry.name).is_some();
                reflected.parameters = native_parameters(entry.name, entry.params);
                reflected.extension = self.registry.function_module(entry.name).map(str::to_owned);
                reflected.return_type = self
                    .registry
                    .function_return_type(entry.name)
                    .map(str::to_owned);
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
                                    signature(&reflected.name)
                                        .and_then(|stub| stub.parameters().nth(position)),
                                )
                            })
                            .collect()
                    },
                    |parameters| native_parameters(&reflected.name, parameters),
                );
                reflected.extension = Some("Core".into());
            }
            CallTarget::Closure(_) => {}
            CallTarget::Magic(magic) => reflected.name.clone_from(&magic.written),
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
                self.reflect_user_signature(
                    reflected,
                    function,
                    Some(&method.declaring),
                    AttributeTarget::Method,
                );
            }
            MethodBody::Native(entry) => {
                use phera_source::deprecations::{ItemKind, kept_notice};
                reflected.deprecated = kept_notice(
                    ItemKind::Method,
                    &format!("{}::{}", method.declaring, method.name),
                )
                .is_some();
                reflected.parameters =
                    native_parameters(&method.signature(&self.code).name, entry.params);
                reflected.extension = self
                    .reflection_class_module(&method.declaring)
                    .map(str::to_owned);
                match entry.return_type {
                    Some(MethodReturn::Required(ty)) => reflected.return_type = Some(ty.into()),
                    Some(MethodReturn::Tentative(ty)) => {
                        reflected.tentative_return_type = Some(ty.into())
                    }
                    None => {}
                }
            }
            MethodBody::Enum(operation) => {
                use crate::classes::enums::EnumMethod;
                reflected.parameters = native_parameters("", operation.parameters());
                reflected.return_type = Some(
                    match operation {
                        EnumMethod::Cases => "array",
                        EnumMethod::From => "static",
                        EnumMethod::TryFrom => "?static",
                    }
                    .into(),
                );
            }
            MethodBody::Closure(operation) => {
                reflected.parameters =
                    native_parameters(&method.signature(&self.code).name, operation.parameters());
                reflected.extension = Some("Core".into());
            }
        }
    }

    fn reflect_user_signature(
        &self,
        reflected: &mut FunctionInfo,
        function: FunctionIndex,
        scope: Option<&str>,
        target: AttributeTarget,
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
            |metadata| source_info(unit, &metadata.declaration),
        ));
        if let Some(metadata) = metadata {
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
        reflected.is_generator = body.is_generator;
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
                let declared_type = param.ty.map(|id| unit.types[id.index()].display.clone());
                ParameterInfo {
                    name: unit.names[param.name.index()].clone(),
                    position,
                    declared_type,
                    by_reference: param.by_reference,
                    can_pass_by_value: !param.by_reference,
                    variadic: param.variadic,
                    optional,
                    promoted: metadata.is_some_and(|meta| meta.promoted),
                    default: metadata
                        .and_then(|meta| meta.default.as_ref())
                        .filter(|_| optional)
                        .map(|default| {
                            super::initializers::reflected_value(default, first, scope).into()
                        }),
                    default_display: param.default_display.clone(),
                    internal: false,
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

fn native_parameters(name: &str, parameters: &[Param]) -> Vec<ParameterInfo> {
    let stub = signature(name);
    parameters
        .iter()
        .copied()
        .enumerate()
        .map(|(position, param)| {
            native_parameter(
                position,
                param,
                stub.and_then(|stub| stub.parameters().nth(position)),
            )
        })
        .collect()
}

fn native_parameter(position: usize, param: Param, stub: Option<StubParameter>) -> ParameterInfo {
    let name = param.ty.name();
    let declared_type = if param.ty == phera_extension_api::ParamType::Untyped {
        None
    } else if param.nullable && name != "mixed" {
        Some(if name.contains('|') {
            format!("{name}|null")
        } else {
            format!("?{name}")
        })
    } else {
        Some(name.into())
    };
    let (default, default_constant, default_display) = match stub {
        Some(parameter) => (
            parameter.default().and_then(|default| match default {
                StubDefault::EnumCase(class, case) => {
                    Some(ParameterDefault::NativeEnumCase { class, case })
                }
                default => default.literal().map(ParameterDefault::Literal),
            }),
            parameter.constant_name().map(str::to_owned),
            (!parameter.expression.is_empty() && parameter.expression != "UNKNOWN")
                .then(|| parameter.expression.to_owned()),
        ),
        None => (
            param
                .default
                .map(|value| ParameterDefault::Literal(value.to_value())),
            param.default_name.map(str::to_owned),
            param.default_name.map(str::to_owned),
        ),
    };
    ParameterInfo {
        name: stub.map_or(param.name, |parameter| parameter.name).into(),
        position,
        declared_type,
        by_reference: stub.map_or(param.by_ref, |parameter| parameter.by_ref),
        can_pass_by_value: !param.requires_ref(),
        variadic: stub.map_or(param.variadic, |parameter| parameter.variadic),
        optional: stub.map_or(param.optional, |parameter| parameter.optional),
        promoted: false,
        default,
        default_constant,
        default_display,
        internal: true,
        attributes: Vec::new(),
    }
}
