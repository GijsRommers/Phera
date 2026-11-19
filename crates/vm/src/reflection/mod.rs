//! `new ReflectionClass(Foo::class)` queries linked classes and immutable source metadata.

use std::rc::Rc;

use phera_bytecode::Visibility;
use phera_extension_api::reflection::{
    AttributeInfo, AttributeSite, AttributeTarget, ClassInfo, ClassSelector, ExtensionInfo,
    FunctionTarget, GeneratorQuery, Initializer, MemberInfo, MemberKind, Members, PropertyHandle,
    PropertyHookKind, PropertyRead, PropertyWrite, ReflectedClassKind, ReflectionContext,
    SourceInfo,
};
use phera_extension_api::{LocalFuture, NativeError, NativeResult};
use phera_value::{Object, PhpArray, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::callback_binding::ArgumentOrigin;
use crate::classes::{Class, Invocation, Method, MethodBody, MethodContext};
use crate::code_table::FunctionIndex;

mod cache;
mod callables;
mod constants;
mod functions;
pub(crate) mod hook_calls;
mod initializers;
mod members;
mod properties;
mod property_hooks;

enum Construction {
    Reflection,
    Attribute(Option<AttributeSite>),
}

#[derive(Default)]
struct AttributeCallSite(Option<AttributeSite>);

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
    fn reflect_generator_function(&self, object: &Object) -> Result<FunctionTarget, NativeError> {
        self.generator_function(object)
    }

    fn reflect_generator(&mut self, object: &Rc<Object>, query: GeneratorQuery) -> NativeResult {
        self.inspect_generator(object, query)
    }

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
        &mut self,
        target: &phera_extension_api::reflection::FunctionTarget,
    ) -> Result<Rc<phera_extension_api::reflection::FunctionInfo>, NativeError> {
        self.cached_function(target)
    }

    fn reflect_invoke(
        &mut self,
        target: phera_extension_api::reflection::FunctionTarget,
        receiver: Option<Rc<Object>>,
        args: PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let callee = self.reflection_function_callee(&target, receiver, false)?;
            if matches!(target, FunctionTarget::PropertyHook { .. }) {
                return Err(hook_calls::ReflectedHookCall::Method { callee, args }.suspend());
            }
            self.invoke_array_callback(callee, &args).await
        })
    }

    fn reflect_closure(
        &mut self,
        target: phera_extension_api::reflection::FunctionTarget,
        receiver: Option<Rc<Object>>,
    ) -> NativeResult {
        use phera_extension_api::reflection::FunctionTarget;
        match &target {
            FunctionTarget::Closure(closure) => {
                return closure
                    .upgrade()
                    .map(Value::Object)
                    .ok_or_else(|| exception("Closure is unavailable"));
            }
            FunctionTarget::ClosureInvoke(_) => {
                return callables::closure_invoke_receiver(receiver).map(Value::Object);
            }
            _ => {}
        }
        let callee = self.reflection_function_callee(&target, receiver, true)?;
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

    fn reflect_class(&mut self, name: &str) -> LocalFuture<'_, Result<Rc<ClassInfo>, NativeError>> {
        if let Some(class) = self.classes.get(name) {
            let info = self.cached_class_info(&class);
            return Box::pin(async move { Ok(info) });
        }
        let name = name.to_owned();
        Box::pin(async move {
            let class = self
                .lookup_class(&name)
                .await?
                .ok_or_else(|| exception(format!("Class \"{name}\" does not exist")))?;
            Ok(self.cached_class_info(&class))
        })
    }

    fn reflect_members(&mut self, class: ClassSelector<'_>, kind: MemberKind) -> Members {
        let class = match class {
            ClassSelector::Linked(id) => self.classes.numbered(id),
            ClassSelector::Name(name) => self.classes.get(name),
        };
        class.map_or_else(Members::default, |class| self.cached_members(&class, kind))
    }

    fn reflect_class_constant(&mut self, class: &str, name: &str) -> NativeResult {
        let constant = self
            .classes
            .get(class)
            .and_then(|class| class.constants.get(name).cloned())
            .ok_or_else(|| exception(format!("Constant {class}::{name} does not exist")))?;
        self.class_constant_with_access(
            &constant.declaring,
            name,
            crate::class_constants::ConstantAccess::Reflect,
        )
    }

    fn reflect_construct(
        &mut self,
        class: &ClassInfo,
        args: Option<PhpArray>,
    ) -> LocalFuture<'_, NativeResult> {
        let class = self.classes.numbered(class.id);
        Box::pin(async move {
            let class = class.ok_or_else(|| exception("Class does not exist"))?;
            self.reflection_construct(class, args, Construction::Reflection)
                .await
        })
    }

    fn reflect_construct_attribute(
        &mut self,
        class: &ClassInfo,
        args: PhpArray,
        site: Option<AttributeSite>,
    ) -> LocalFuture<'_, NativeResult> {
        let class = self.classes.numbered(class.id);
        Box::pin(async move {
            let class = class.ok_or_else(|| exception("Class does not exist"))?;
            self.reflection_construct(class, Some(args), Construction::Attribute(site))
                .await
        })
    }

    fn reflect_evaluate(&mut self, initializer: Initializer) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let function = match self.reflection_initializer_code(&initializer).await? {
                initializers::CompiledInitializer::Function(function) => function,
                initializers::CompiledInitializer::Literal(value) => return Ok(value),
            };
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

    fn reflect_property_hook(
        &self,
        member: &MemberInfo,
        kind: PropertyHookKind,
    ) -> Option<FunctionTarget> {
        self.reflection_property_hook(member, kind)
    }

    fn reflect_hook_prototype(
        &self,
        property: PropertyHandle,
        kind: PropertyHookKind,
    ) -> Option<FunctionTarget> {
        self.reflection_hook_prototype(property, kind)
    }

    fn reflect_property_settable_type(&self, member: &MemberInfo) -> Option<String> {
        self.reflection_property_settable_type(member)
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
        mode: PropertyWrite,
    ) -> NativeResult {
        self.reflection_property_write(member, object, value, mode)
    }
}

impl Vm {
    pub(crate) fn attribute_call_site(&self) -> Option<&AttributeSite> {
        if self.argument_origin == ArgumentOrigin::Attribute {
            self.state.get::<AttributeCallSite>()?.0.as_ref()
        } else {
            None
        }
    }

    fn reflection_class_module(&self, name: &str) -> Option<&'static str> {
        self.registry.class_module(name).or_else(|| {
            matches!(
                name,
                "stdClass" | "Closure" | "Generator" | "ClosedGeneratorException"
            )
            .then_some("Core")
        })
    }

    fn reflection_class_info(&self, class: &Class) -> ClassInfo {
        use crate::classes::class::ClassOrigin;
        let kind = match class.kind {
            phera_bytecode::ClassKind::Class | phera_bytecode::ClassKind::Anonymous => {
                ReflectedClassKind::Class
            }
            phera_bytecode::ClassKind::Enum => ReflectedClassKind::Enum,
            phera_bytecode::ClassKind::Trait => ReflectedClassKind::Trait,
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
        let mut trait_aliases = Vec::new();
        let mut source = None;
        let mut attributes = Vec::new();
        if let Some((first, index)) = class.declaration {
            let unit = self.code.unit(first);
            if let Some(declaration) = unit.classes.get(index.index()) {
                source = Some(source_info(unit, &declaration.metadata));
                trait_aliases = self.reflection_trait_aliases(class, declaration);
                attributes = self.reflection_attributes(
                    first,
                    &declaration.metadata.attributes,
                    AttributeTarget::Class,
                    Some(&class.name),
                );
            }
        }
        ClassInfo {
            id: class.index.number(),
            name: class.name.clone(),
            kind,
            source,
            extension: self.reflection_class_module(&class.name).map(str::to_owned),
            parent: class.parent.as_ref().map(|parent| parent.name.clone()),
            interfaces: crate::class_listings::interfaces_in_php_order(class)
                .into_iter()
                .map(|interface| interface.name.clone())
                .collect(),
            traits: class.traits.clone(),
            trait_aliases,
            enum_backing: class
                .enum_info
                .as_ref()
                .and_then(|info| info.backing)
                .map(|backing| match backing {
                    phera_bytecode::EnumBacking::Int => "int",
                    phera_bytecode::EnumBacking::String => "string",
                }),
            modifiers: flags(class.is_abstract && kind == ReflectedClassKind::Class, 64)
                | flags(class.is_final || class.origin == ClassOrigin::Closure, 32)
                | flags(class.is_readonly, 65536),
            is_abstract: class.is_abstract
                || kind == ReflectedClassKind::Interface
                    && (class.methods.values().any(|method| method.is_abstract)
                        || class.defaults.iter().any(|property| {
                            property
                                .hooks
                                .as_ref()
                                .is_some_and(|hooks| hooks.methods().any(|hook| hook.is_abstract))
                        })),
            instantiable: concrete && constructor_public && class.origin != ClassOrigin::Closure,
            cloneable,
            iterable: concrete && class.is_a("Traversable"),
            anonymous: class.kind == phera_bytecode::ClassKind::Anonymous,
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
    ) -> Vec<Rc<AttributeInfo>> {
        let first = self.code.facts(unit_function).unit_first;
        let file = &self.code.unit(unit_function).file;
        attributes
            .iter()
            .map(|attribute| {
                Rc::new(AttributeInfo {
                    validation_error: attribute.validation_error.clone(),
                    site: Some(AttributeSite {
                        file: file.clone(),
                        line: attribute.line,
                        strict_types: attribute.strict_types,
                    }),
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
            })
            .collect()
    }

    async fn reflection_construct(
        &mut self,
        class: Rc<Class>,
        args: Option<PhpArray>,
        construction: Construction,
    ) -> NativeResult {
        use phera_bytecode::ClassKind;
        if matches!(class.kind, ClassKind::Trait | ClassKind::Enum) {
            return Err(NativeError::error(format!(
                "Cannot instantiate {} {}",
                class.kind.description().to_ascii_lowercase(),
                class.name
            )));
        }
        let args = match args {
            Some(args) => args,
            None => {
                if class.has_native_layout && class.is_final {
                    return Err(exception(format!(
                        "Class {} is an internal class marked as final that cannot be instantiated without invoking its constructor",
                        class.name
                    )));
                }
                Self::check_factory_class(&class)?;
                return self.create_linked_object(class, PhpArray::new());
            }
        };
        // Closure exposes a private placeholder for reflection, while Zend's
        // object allocator has no callable constructor for argument forwarding.
        if class.origin == crate::classes::class::ClassOrigin::Closure && !args.is_empty() {
            return Err(exception(
                "Class Closure does not have a constructor, so you cannot pass any constructor arguments",
            ));
        }
        Self::check_factory_class(&class)?;
        let created = self.create_linked_object(class.clone(), PhpArray::new())?;
        let constructor = class.constructor.clone();
        match &constructor {
            Some(method) if method.visibility != Visibility::Public => {
                if matches!(construction, Construction::Attribute(_)) {
                    let visibility = match method.visibility {
                        Visibility::Private => "private",
                        _ => "protected",
                    };
                    return Err(NativeError::error(format!(
                        "Call to {visibility} {}::__construct() from global scope",
                        method.declaring
                    )));
                }
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
            None if !args.is_empty() && matches!(construction, Construction::Reflection) => {
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
            if let Construction::Attribute(Some(site)) = construction {
                let previous = self
                    .state
                    .get_or_default::<AttributeCallSite>()
                    .0
                    .replace(site);
                let result = self
                    .invoke_array_callback_origin(callee, &args, ArgumentOrigin::Attribute)
                    .await;
                self.state.get_or_default::<AttributeCallSite>().0 = previous;
                result?;
            } else {
                self.invoke_array_callback(callee, &args).await?;
            }
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
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
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
    unit: &phera_bytecode::CompiledUnit,
    metadata: &phera_bytecode::reflection::DeclarationMetadata,
) -> SourceInfo {
    let line = |offset| {
        unit.reflection_lines
            .as_ref()
            .map_or(0, |lines| lines.line(offset) as u32)
    };
    let comment = metadata.doc_comment;
    let doc_comment = (comment.start < comment.end)
        .then(|| {
            unit.reflection_source
                .as_deref()?
                .get(comment.start..comment.end)
        })
        .flatten()
        .map(<[u8]>::to_vec);
    SourceInfo {
        file: unit.file.clone(),
        start_line: line(metadata.span.start),
        end_line: line(metadata.span.end.saturating_sub(1)),
        doc_comment,
    }
}
