//! Native and source declarations inherit the same runtime tables before objects are allocated.

use std::{rc::Rc, sync::Arc};

use phera_bytecode::{ClassId, ClassKind, CompiledUnit, Visibility};
use phera_extension_api::NativeError;

use super::{Class, Method, MethodBody};
use crate::Vm;

impl Vm {
    /// Hoist only declarations whose dependencies are available; deferred ones execute normally.
    pub(crate) fn hoist_classes(&mut self, unit: &Arc<CompiledUnit>) -> Result<(), NativeError> {
        for &index in &unit.hoisted_classes {
            let declaration = &unit.classes[index.index()];
            if declaration.interfaces.is_empty()
                && declaration.parent.is_none_or(|parent| {
                    self.classes
                        .get(&unit.symbols[parent.index()].written)
                        .is_some()
                })
                && self.classes.get(&declaration.name).is_none()
            {
                if let Err(error) = self.link_class(unit.clone(), index) {
                    if crate::autoload::ClassMiss::is(&error) {
                        continue;
                    }
                    return Err(error);
                }
                self.prelinked_classes
                    .insert((Arc::as_ptr(unit) as usize, index.index()));
            }
        }
        Ok(())
    }

    /// Runtime declarations are not idempotent; only the exact prelinked instruction is skipped.
    pub(crate) fn declare_class(&mut self, index: ClassId) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("class frame"))?
            .unit()
            .clone();
        if self
            .prelinked_classes
            .remove(&(Arc::as_ptr(&unit) as usize, index.index()))
        {
            return Ok(());
        }
        let declaration = &unit.classes[index.index()];
        if declaration.kind == ClassKind::Anonymous && self.classes.get(&declaration.name).is_some()
        {
            return Ok(());
        }
        self.link_class(unit, index)
    }

    /// Copy inherited tables and validate structural dependencies before publishing a class.
    fn link_class(&mut self, unit: Arc<CompiledUnit>, index: ClassId) -> Result<(), NativeError> {
        self.link_class_body(unit.clone(), index)
            .map_err(|error| super::declaration_errors::link_failure(&unit, index, error))
    }

    /// Structural declaration failures are fatal; missing dependencies remain catchable Errors.
    fn link_class_body(
        &mut self,
        unit: Arc<CompiledUnit>,
        declaration_index: ClassId,
    ) -> Result<(), NativeError> {
        let declaration = &unit.classes[declaration_index.index()];
        let name = &declaration.name;
        if let Some(existing) = self.classes.get(name) {
            return Err(NativeError::error(format!(
                "Cannot redeclare class {}",
                existing.name
            )));
        }
        let parent = declaration
            .parent
            .map(|symbol| {
                let parent_name = &unit.symbols[symbol.index()].written;
                self.classes
                    .get(parent_name)
                    .ok_or_else(|| Self::class_miss(parent_name, "Class"))
            })
            .transpose()?;
        if let Some(parent) = &parent {
            let display = name.split('\0').next().unwrap_or(name);
            if parent.kind == ClassKind::Enum {
                return Err(NativeError::error(format!(
                    "Class {display} cannot extend enum {}",
                    parent.name
                )));
            }
            if parent.is_final {
                return Err(NativeError::error(format!(
                    "Class {display} cannot extend final class {}",
                    parent.name
                )));
            }
            if parent.kind == ClassKind::Interface {
                return Err(NativeError::error(format!(
                    "Class {display} cannot extend interface {}",
                    parent.name
                )));
            }
            if parent.kind == ClassKind::Trait {
                return Err(NativeError::error(format!(
                    "Class {display} cannot extend trait {}",
                    parent.name
                )));
            }
            if declaration.is_readonly != parent.is_readonly {
                let (child, inherited) = if declaration.is_readonly {
                    ("Readonly", "non-readonly")
                } else {
                    ("Non-readonly", "readonly")
                };
                return Err(NativeError::error(format!(
                    "{child} class {display} cannot extend {inherited} class {}",
                    parent.name
                )));
            }
        }
        let mut interfaces = declaration
            .interfaces
            .iter()
            .map(|symbol| {
                let interface_name = &unit.symbols[symbol.index()].written;
                let interface = self
                    .classes
                    .get(interface_name)
                    .ok_or_else(|| Self::class_miss(interface_name, "Interface"))?;
                if interface.kind != ClassKind::Interface {
                    return Err(NativeError::error(format!(
                        "{name} cannot implement {interface_name} - it is not an interface"
                    )));
                }
                Ok(interface)
            })
            .collect::<Result<Vec<_>, NativeError>>()?;
        let mut seen = std::collections::HashSet::new();
        if let Some(duplicate) = interfaces
            .iter()
            .find(|interface| !seen.insert(interface.index))
        {
            let kind = declaration.kind.description();
            return Err(NativeError::error(format!(
                "{kind} {name} cannot implement previously implemented interface {}",
                duplicate.name
            )));
        }
        self.link_enum_interfaces(declaration, &mut interfaces)?;
        if declaration.kind != ClassKind::Interface
            && interfaces
                .iter()
                .any(|interface| interface.is_a("Throwable"))
            && parent
                .as_ref()
                .is_none_or(|parent| !parent.is_a("Throwable"))
        {
            if declaration.kind == ClassKind::Enum {
                return Err(NativeError::error(format!(
                    "Enum {name} cannot implement interface Throwable"
                )));
            }
            return Err(NativeError::error(format!(
                "Class {name} cannot implement interface Throwable, extend Exception or \
                 Error instead"
            )));
        }
        use super::operations::ClassOperations;
        let operations = ClassOperations::link(name, parent.as_deref(), &interfaces);
        if !matches!(declaration.kind, ClassKind::Trait | ClassKind::Interface) {
            let name = super::trait_methods::display(name);
            let iterator = operations.has(ClassOperations::ITERATOR);
            let aggregate = operations.has(ClassOperations::AGGREGATE);
            if iterator && aggregate {
                return Err(NativeError::error(format!(
                    "Class {name} cannot implement both Iterator and IteratorAggregate at the same time"
                )));
            }
            if operations.has(ClassOperations::TRAVERSABLE)
                && !iterator
                && !aggregate
                && !declaration.is_abstract
                && parent.as_ref().is_none_or(|parent| {
                    !parent.operations.has(ClassOperations::TRAVERSABLE) || parent.is_abstract
                })
            {
                let kind = declaration.kind.description();
                return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                    crate::error_traces::DeclarationFailure {
                        message: format!(
                            "{kind} {name} must implement interface Traversable as part of either Iterator or IteratorAggregate"
                        ),
                        file: "Unknown".into(),
                        line: 0,
                    },
                ))));
            }
        }
        let mut methods = parent
            .as_ref()
            .map(|class| class.methods.clone())
            .unwrap_or_default();
        for method in &declaration.methods {
            let method_name = &unit.names[method.name.index()];
            let linked = Rc::new(Method {
                signature: std::cell::OnceCell::new(),
                name: method_name.clone(),
                declaring: name.clone(),
                visibility: method.visibility,
                is_static: method.is_static,
                is_final: method.is_final,
                is_abstract: method.is_abstract,
                body: MethodBody::User(self.code.index_in(&unit, Some(method.function.index()))),
            });
            if let Some(inherited) = methods.get(&method_name.to_ascii_lowercase()) {
                super::inheritance_checks::check_override(name, inherited, &linked)
                    .map_err(|error| self.magic_override_error(&linked, error))?;
                self.check_method_contract(inherited, &linked, &unit)?;
            }
            methods.insert(method_name.to_ascii_lowercase(), linked);
        }
        let mut enum_info = self.install_enum_methods(declaration, &mut methods)?;
        let traits = self.resolve_traits(declaration)?;
        if !traits.is_empty() {
            self.compose_trait_methods(declaration, &unit, &traits, &mut methods)?;
        }
        if enum_info.is_some() {
            super::enums::check_magic_methods(name, &methods)?;
        }
        if declaration.kind != ClassKind::Trait
            && let Some(method) = methods.get("__tostring")
        {
            if method.visibility != Visibility::Public
                && let MethodBody::User(function) = method.body
            {
                let file = self.code.unit(function).file.clone();
                let line = self.code.function(function).definition_line as usize;
                let message = format!(
                    "Access level to {}::{}() must be public (as in class Stringable)",
                    phera_value::class_display_name(&method.declaring),
                    method.name
                );
                return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                    crate::error_traces::DeclarationFailure {
                        message,
                        file,
                        line,
                    },
                ))));
            }
            if !interfaces
                .iter()
                .any(|interface| interface.is_a("Stringable"))
                && !parent
                    .as_ref()
                    .is_some_and(|parent| parent.is_a("Stringable"))
                && let Some(stringable) = self.classes.get("Stringable")
            {
                interfaces.push(stringable);
            }
        }
        for interface in &interfaces {
            for (method_name, method) in &interface.methods {
                if let Some(implementation) = methods.get(method_name) {
                    let native = matches!(method.body, MethodBody::Native(entry) if entry.return_type.is_some());
                    if native
                        || matches!(declaration.kind, ClassKind::Enum | ClassKind::Anonymous)
                        || !traits.is_empty()
                    {
                        super::inheritance_checks::check_override(
                            super::trait_methods::display(name),
                            method,
                            implementation,
                        )
                        .map_err(|error| {
                            if native {
                                super::declaration_errors::method_failure(
                                    &self.code,
                                    implementation,
                                    error,
                                )
                            } else {
                                error
                            }
                        })?;
                    }
                    self.check_method_contract(method, implementation, &unit)?;
                }
                methods
                    .entry(method_name.clone())
                    .or_insert_with(|| method.clone());
            }
        }
        super::inheritance_checks::check_abstract(
            name,
            declaration.kind,
            declaration.is_abstract,
            declaration.is_final,
            &methods,
            &self.registry,
        )?;
        let hooks = super::operations::user_hooks(&methods, parent.as_deref(), operations);
        let mut constants =
            self.link_class_constants(declaration, &unit, parent.as_ref(), &interfaces)?;
        let (mut defaults, mut static_properties) =
            self.link_class_properties(declaration, &unit, parent.as_ref())?;
        if !traits.is_empty() {
            self.compose_trait_members(
                declaration,
                &unit,
                &traits,
                &mut defaults,
                &mut static_properties,
                &mut constants,
            )?;
        }
        self.link_property_contracts(declaration, &unit, &interfaces, &mut defaults)?;
        if let Some(enum_info) = &mut enum_info {
            self.link_enum_members(
                declaration,
                &unit,
                enum_info,
                &mut defaults,
                &static_properties,
                &mut constants,
            )?;
        }
        let allows_dynamic_properties = declaration.allows_dynamic_properties
            || parent
                .as_ref()
                .is_some_and(|parent| parent.allows_dynamic_properties);
        let is_throwable = parent.as_ref().is_some_and(|parent| parent.is_throwable)
            || interfaces.iter().any(|interface| interface.is_throwable);
        self.classes.order.push(name.clone());
        let index = self.classes.next_index();
        for method in &declaration.methods {
            let function = self.code.index_in(&unit, Some(method.function.index()));
            self.code.set_declaring_class(function, index);
        }
        for property in &declaration.properties {
            if let Some(hooks) = &property.hooks {
                for hook in hooks.get.iter().chain(&hooks.set) {
                    let function = self.code.index_in(&unit, Some(hook.function.index()));
                    self.code.set_declaring_class(function, index);
                }
            }
        }
        let declaration_location = Some((self.code.index_in(&unit, None), declaration.line));
        self.classes.insert(
            name.to_ascii_lowercase(),
            Rc::new_cyclic(|identity| Class {
                index,
                property_access: hooks.as_ref().and_then(|hooks| hooks.property_access),
                hooks,
                operations,
                origin: super::class::ClassOrigin::User,
                identity: identity.clone(),
                declaration_matches: std::cell::RefCell::new(std::collections::HashMap::new()),
                ancestors: super::class::ancestor_names(&parent, &interfaces),
                name_value: phera_value::Value::from(name.as_str()),
                name: name.clone(),
                has_native_layout: parent.as_ref().is_some_and(|class| class.has_native_layout),
                parent,
                interfaces,
                traits: traits
                    .into_iter()
                    .map(|used_trait| used_trait.name.clone())
                    .collect(),
                enum_info,
                kind: declaration.kind,
                is_abstract: declaration.is_abstract,
                is_throwable,
                is_final: declaration.is_final,
                is_readonly: declaration.is_readonly,
                allows_dynamic_properties,
                constructor: methods.get("__construct").cloned(),
                magic: super::magic::MagicMethods::link(&methods),
                methods,
                method_table: std::cell::OnceCell::new(),
                property_template: super::class::property_template(&defaults),
                defaults,
                static_properties,
                constants,
                declaration: declaration_location,
            }),
        );
        Ok(())
    }
}
