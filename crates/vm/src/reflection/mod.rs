//! `new ReflectionClass(Foo::class)` queries linked classes and immutable source metadata.

use std::rc::Rc;

use phera_bytecode::Visibility;
use phera_extension_api::reflection::{
    AttributeInfo, AttributeTarget, ClassInfo, ExtensionInfo, Initializer, MemberInfo, MemberKind,
    PropertyRead, ReflectedClassKind, ReflectionContext, SourceInfo,
};
use phera_extension_api::{LocalFuture, NativeError, NativeResult};
use phera_value::{Object, PhpArray, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};
use crate::classes::{Class, Invocation, Method, MethodBody, MethodContext};
use crate::code_table::FunctionIndex;

mod callables;
mod constants;
mod functions;
mod initializers;
mod members;
mod properties;

pub(super) fn exception(message: impl Into<String>) -> NativeError {
    NativeError::throw("ReflectionException", message)
}

fn visibility_bits(visibility: Visibility) -> i64 {
    match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    }
}

fn flags(enabled: bool, bits: i64) -> i64 {
    if enabled { bits } else { 0 }
}

impl ReflectionContext for Vm {
    fn reflect_constant(
        &self,
        name: &str,
    ) -> Result<phera_extension_api::reflection::ConstantInfo, NativeError> {
        self.reflection_constant(name)
    }

    fn reflect_constant_value(&self, name: &str) -> NativeResult {
        self.constants
            .get(name)
            .map(|constant| constant.value.clone())
            .ok_or_else(|| exception(format!("Constant \"{name}\" does not exist")))
    }

    fn reflect_function(
        &self,
        target: &phera_extension_api::reflection::FunctionTarget,
    ) -> Result<phera_extension_api::reflection::FunctionInfo, NativeError> {
        self.reflection_function(target)
    }

    fn reflect_invoke(
        &mut self,
        target: phera_extension_api::reflection::FunctionTarget,
        receiver: Option<Rc<Object>>,
        args: PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let callee = self.reflection_function_callee(&target, receiver)?;
            self.invoke_array_callback(callee, &args).await
        })
    }

    fn reflect_closure(
        &mut self,
        target: phera_extension_api::reflection::FunctionTarget,
        receiver: Option<Rc<Object>>,
    ) -> NativeResult {
        let callee = self.reflection_function_callee(&target, receiver)?;
        self.closure_from_callee(callee).map(Value::Object)
    }

    fn reflect_closure_uses(
        &self,
        target: &phera_extension_api::reflection::FunctionTarget,
    ) -> NativeResult {
        self.reflection_closure_uses(target)
    }

    fn reflect_statics(
        &mut self,
        target: &phera_extension_api::reflection::FunctionTarget,
    ) -> NativeResult {
        self.reflection_statics(target)
    }

    fn reflect_prototype(&self, method: &MemberInfo) -> Option<MemberInfo> {
        self.reflection_prototype(method)
    }

    fn reflect_extension(&self, name: &str) -> Option<ExtensionInfo> {
        let name = self
            .registry
            .extensions()
            .find(|module| module.eq_ignore_ascii_case(name))?;
        let mut constants = PhpArray::new();
        for (key, module) in self.registry.constant_names() {
            if module == name
                && let Some(value) = self.registry.get_constant(key)
            {
                constants.insert(phera_value::ArrayKey::str(key), value.to_value());
            }
        }
        Some(ExtensionInfo {
            name: name.into(),
            version: self
                .registry
                .module_version(name)
                .unwrap_or(phera_extension_api::PHP_VERSION)
                .into(),
            classes: self
                .registry
                .classes()
                .filter(|entry| self.registry.class_module(entry.name) == Some(name))
                .map(|entry| entry.name.into())
                .collect(),
            functions: self
                .registry
                .functions()
                .filter(|entry| self.registry.function_module(entry.name) == Some(name))
                .map(|entry| entry.name.into())
                .collect(),
            constants,
        })
    }

    fn reflect_dynamic_property_write(
        &mut self,
        object: Rc<Object>,
        name: &str,
        mut value: Value,
    ) -> NativeResult {
        let key = self.property_key(&object, name)?;
        let mut path = crate::inline_stack::InlineStack::new();
        path.push(crate::path_update::PathKey::Property(key.clone()));
        let handled = self.validate_property_mutation(
            Value::Object(object.clone()),
            &path,
            &mut value,
            crate::access_mode::Mutation::Assign,
        )?;
        if !handled {
            crate::property_storage::write(&object, key, value.deref().clone());
        }
        Ok(Value::Null)
    }

    fn reflect_class(&mut self, name: &str) -> LocalFuture<'_, Result<ClassInfo, NativeError>> {
        let name = name.to_owned();
        Box::pin(async move {
            let class = self
                .lookup_class(&name)
                .await?
                .ok_or_else(|| exception(format!("Class \"{name}\" does not exist")))?;
            Ok(self.reflection_class_info(&class))
        })
    }

    fn reflect_members(&self, class: &str, kind: MemberKind) -> Vec<MemberInfo> {
        self.classes
            .get(class)
            .map_or_else(Vec::new, |class| match kind {
                MemberKind::Method => self.reflection_methods(&class),
                MemberKind::Property => self.reflection_properties(&class),
                MemberKind::Constant => self.reflection_constants(&class),
            })
    }

    fn reflect_class_constant(&mut self, class: &str, name: &str) -> NativeResult {
        let constant = self
            .classes
            .get(class)
            .and_then(|class| class.constants.get(name).cloned())
            .ok_or_else(|| exception(format!("Constant {class}::{name} does not exist")))?;
        self.evaluate_class_constant(&constant, name)
    }

    fn reflect_construct(
        &mut self,
        class: &str,
        args: Option<PhpArray>,
    ) -> LocalFuture<'_, NativeResult> {
        let class = self.classes.get(class);
        Box::pin(async move {
            let class = class.ok_or_else(|| exception("Class does not exist"))?;
            self.reflection_construct(class, args).await
        })
    }

    fn reflect_evaluate(&mut self, initializer: Initializer) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let function = self.reflection_initializer_code(&initializer).await?;
            let callee =
                if let Some(scope) = initializer.scope.and_then(|name| self.classes.get(&name)) {
                    self.code.set_declaring_class(function, scope.index);
                    let method = Rc::new(Method {
                        signature: std::cell::OnceCell::new(),
                        name: "{reflection initializer}".into(),
                        declaring: scope.name.clone(),
                        visibility: Visibility::Public,
                        is_static: true,
                        is_final: false,
                        is_abstract: false,
                        body: MethodBody::User(function),
                    });
                    self.reflection_callee(scope.clone(), scope, method, None)
                } else {
                    self.function_callee(function)
                };
            self.invoke_resolved_callable(callee, []).await
        })
    }

    fn reflect_property_read(
        &mut self,
        member: &MemberInfo,
        object: Option<Rc<Object>>,
        mode: PropertyRead,
    ) -> NativeResult {
        self.reflection_property_read(member, object, mode)
    }

    fn reflect_property_write(
        &mut self,
        member: &MemberInfo,
        object: Option<Rc<Object>>,
        value: Value,
    ) -> NativeResult {
        self.reflection_property_write(member, object, value)
    }
}

impl Vm {
    fn reflection_class_info(&self, class: &Class) -> ClassInfo {
        use crate::classes::class::ClassOrigin;
        let kind = match class.kind {
            phera_bytecode::ClassKind::Class => ReflectedClassKind::Class,
            phera_bytecode::ClassKind::Interface => ReflectedClassKind::Interface,
        };
        let constructor_public = class
            .constructor
            .as_ref()
            .is_none_or(|method| method.visibility == Visibility::Public);
        let concrete = !class.is_abstract && kind == ReflectedClassKind::Class;
        let clone_hook = class.hooks.as_ref().and_then(|hooks| hooks.clone);
        let clone_method = class.methods.get("__clone");
        let cloneable = concrete
            && !class.is_throwable
            && !matches!(
                clone_hook,
                Some(phera_extension_api::NativeClone::Forbidden)
            )
            && clone_method.is_none_or(|method| method.visibility == Visibility::Public);
        let mut source = None;
        let mut attributes = Vec::new();
        if let Some((first, index)) = class.declaration {
            let unit = self.code.unit(first);
            if let Some(declaration) = unit.classes.get(index.index()) {
                source = Some(source_info(&unit.file, &declaration.metadata));
                attributes = self.reflection_attributes(
                    first,
                    &declaration.metadata.attributes,
                    AttributeTarget::Class,
                    Some(&class.name),
                );
            }
        }
        ClassInfo {
            name: class.name.clone(),
            kind,
            source,
            extension: self
                .registry
                .class_module(&class.name)
                .or_else(|| matches!(class.name.as_str(), "stdClass" | "Closure").then_some("Core"))
                .map(str::to_owned),
            parent: class.parent.as_ref().map(|parent| parent.name.clone()),
            interfaces: crate::class_listings::interfaces_in_php_order(class)
                .into_iter()
                .map(|interface| interface.name.clone())
                .collect(),
            traits: Vec::new(),
            trait_aliases: Vec::new(),
            modifiers: flags(class.is_abstract && kind == ReflectedClassKind::Class, 64)
                | flags(class.is_final || class.origin == ClassOrigin::Closure, 32)
                | flags(class.is_readonly, 65536),
            is_abstract: class.is_abstract
                || kind == ReflectedClassKind::Interface
                    && class.methods.values().any(|method| method.is_abstract),
            instantiable: concrete && constructor_public && class.origin != ClassOrigin::Closure,
            cloneable,
            iterable: concrete && class.is_a("Traversable"),
            anonymous: false,
            constructor: class
                .constructor
                .as_ref()
                .map(|method| method.declaring.clone()),
            attributes,
        }
    }

    fn reflection_attributes(
        &self,
        unit_function: FunctionIndex,
        attributes: &[phera_bytecode::reflection::Attribute],
        target: AttributeTarget,
        class: Option<&str>,
    ) -> Vec<AttributeInfo> {
        let first = self.code.facts(unit_function).unit_first;
        attributes
            .iter()
            .map(|attribute| AttributeInfo {
                validation_error: attribute.validation_error.clone(),
                name: attribute.name.clone(),
                arguments: attribute
                    .arguments
                    .iter()
                    .map(|argument| {
                        (
                            argument.name.clone(),
                            initializers::reflected_value(&argument.initializer, first, class),
                        )
                    })
                    .collect(),
                target,
            })
            .collect()
    }

    async fn reflection_construct(
        &mut self,
        class: Rc<Class>,
        args: Option<PhpArray>,
    ) -> NativeResult {
        use phera_extension_api::Context;
        let args = match args {
            Some(args) => args,
            None => {
                if class.has_native_layout && class.is_final {
                    return Err(exception(format!(
                        "Class {} is an internal class marked as final that cannot be instantiated without invoking its constructor",
                        class.name
                    )));
                }
                return self.create_object(&class.name, PhpArray::new());
            }
        };
        let created = self.create_object(&class.name, PhpArray::new())?;
        let constructor = class.constructor.clone();
        match &constructor {
            Some(method) if method.visibility != Visibility::Public => {
                if method.declaring == class.name || method.visibility == Visibility::Protected {
                    return Err(exception(format!(
                        "Access to non-public constructor of class {}",
                        class.name
                    )));
                }
                let visibility = if method.visibility == Visibility::Private {
                    "private"
                } else {
                    "protected"
                };
                return Err(NativeError::error(format!(
                    "Call to {visibility} {}::__construct() from scope {}",
                    method.declaring, class.name
                )));
            }
            None if !args.is_empty() => {
                return Err(exception(format!(
                    "Class {} does not have a constructor, so you cannot pass any constructor arguments",
                    class.name
                )));
            }
            _ => {}
        }
        if let (Some(method), Value::Object(object)) = (constructor, &created) {
            let scope = self
                .classes
                .get(&method.declaring)
                .ok_or_else(|| exception("Constructor scope does not exist"))?;
            let callee = self.reflection_callee(class, scope, method, Some(object.clone()));
            self.invoke_array_callback(callee, &args).await?;
        }
        Ok(created)
    }

    fn reflection_callee(
        &self,
        called: Rc<Class>,
        scope: Rc<Class>,
        method: Rc<Method>,
        receiver: Option<Rc<Object>>,
    ) -> Callee {
        let signature = method.signature(&self.code);
        Callee {
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Internal,
            target: CallTarget::Method {
                context: MethodContext {
                    receiver: if method.is_static { None } else { receiver },
                    scope,
                    called,
                    invocation: Invocation::Method,
                },
                method,
            },
        }
    }
}

fn source_info(
    file: &str,
    metadata: &phera_bytecode::reflection::DeclarationMetadata,
) -> SourceInfo {
    SourceInfo {
        file: file.to_owned(),
        start_line: metadata.start_line,
        end_line: metadata.end_line,
        doc_comment: metadata.doc_comment.clone(),
    }
}
