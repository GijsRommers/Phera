//! `public string $name { get => $this->name; set => trim($value); }`
//! compiles two ordinary class-scoped functions without publishing methods.

use phera_ast as ast;
use phera_bytecode::{PropertyHook, PropertyHooks};
use phera_source::attributes::AttributeTarget;

use super::builtin_attributes::AttributePlacement;
use super::functions::{FunctionBody, FunctionSource};
use crate::ir::Instruction;
use crate::{CompileError, Compiler};

pub(super) struct HookedDeclaration<'a> {
    pub class: &'a str,
    pub trait_scope: Option<&'a str>,
    pub kind: ast::ClassKind,
    pub property: &'a str,
    pub modifiers: &'a ast::Modifiers,
    pub readonly: bool,
    pub ty: Option<&'a ast::Type>,
    pub default: Option<&'a ast::Expression>,
    pub hooks: &'a [ast::PropertyHook],
    pub has_hook_list: bool,
    pub has_parent: bool,
    pub line: usize,
}

impl Compiler<'_> {
    pub(super) fn compile_property_metadata(
        &mut self,
        declaration: HookedDeclaration<'_>,
    ) -> Result<Option<Box<PropertyHooks<usize>>>, CompileError> {
        let modifiers = declaration.modifiers;
        if modifiers.is_final && modifiers.is_abstract {
            return Err(self.error(
                "Cannot use the final modifier on an abstract property",
                declaration.line,
            ));
        }
        if !declaration.has_hook_list {
            if declaration.kind == ast::ClassKind::Interface {
                return Err(self.error(
                    "Interfaces may only include hooked properties",
                    declaration.line,
                ));
            }
            return Ok(None);
        }
        self.compile_property_hooks(declaration).map(Some)
    }

    fn compile_property_hooks(
        &mut self,
        declaration: HookedDeclaration<'_>,
    ) -> Result<Box<PropertyHooks<usize>>, CompileError> {
        let property = format!("{}::${}", declaration.class, declaration.property);
        let interface = declaration.kind == ast::ClassKind::Interface;
        if declaration.readonly {
            return Err(self.error("Hooked properties cannot be readonly", declaration.line));
        }
        if declaration.hooks.is_empty() {
            return Err(self.error("Property hook list must not be empty", declaration.line));
        }
        if interface {
            let invalid = if declaration.modifiers.is_final {
                Some("Property in interface cannot be final")
            } else if declaration.modifiers.is_abstract {
                Some(
                    "Property in interface cannot be explicitly abstract. All interface members are implicitly abstract",
                )
            } else if matches!(
                declaration.modifiers.visibility,
                Some(ast::Visibility::Private | ast::Visibility::Protected)
            ) {
                Some("Property in interface cannot be protected or private")
            } else {
                None
            };
            if let Some(message) = invalid {
                return Err(self.error(message, declaration.line));
            }
        }
        let mut result = PropertyHooks {
            get: None,
            set: None,
            is_virtual: true,
        };
        for hook in declaration.hooks {
            let line = self.lines.line(hook.span.start);
            let name = &hook.name.name;
            if let Some(modifier) = &hook.invalid_modifier {
                let message = if modifier.name == "final" {
                    "Multiple final modifiers are not allowed".to_owned()
                } else {
                    format!(
                        "Cannot use the {} modifier on a property hook",
                        modifier.name
                    )
                };
                return Err(self.error(message, self.lines.line(modifier.span.start)));
            }
            if declaration.modifiers.is_static {
                return Err(self.error("Cannot declare hooks for static property", line));
            }
            let private = declaration.modifiers.visibility == Some(ast::Visibility::Private);
            let abstract_hook = matches!(hook.body, ast::HookBody::None);
            if let Some(message) = match (hook.is_final, private, abstract_hook) {
                (true, true, _) => Some("Property hook cannot be both final and private"),
                (_, _, false) if interface => Some("Abstract property hook cannot have body"),
                (_, _, true) if !interface && !declaration.modifiers.is_abstract => {
                    Some("Non-abstract property hook must have a body")
                }
                (_, true, true) => Some("Property hook cannot be both abstract and private"),
                (true, _, true) => Some("Property hook cannot be both abstract and final"),
                _ => None,
            } {
                return Err(self.error(message, line));
            }
            let (entry, setter) = if name.eq_ignore_ascii_case("get") {
                (&mut result.get, false)
            } else if name.eq_ignore_ascii_case("set") {
                (&mut result.set, true)
            } else {
                return Err(self.error(format!("Unknown hook \"{name}\" for property {property}, expected \"get\" or \"set\""), line));
            };
            if entry.is_some() {
                return Err(self.error(format!("Cannot redeclare property hook \"{name}\""), line));
            }
            let implicit = ast::Parameter {
                attributes: Vec::new(),
                doc_comment: None,
                modifiers: ast::Modifiers::default(),
                ty: declaration.ty.cloned(),
                by_reference: false,
                variadic: false,
                name: "value".into(),
                default: None,
                hooks: Vec::new(),
                has_hook_list: false,
                span: hook.span,
            };
            let parameters = if setter {
                if hook.has_parameter_list {
                    self.validate_set_parameters(hook, &property, declaration.ty.is_some(), line)?;
                    &hook.parameters
                } else {
                    std::slice::from_ref(&implicit)
                }
            } else {
                if hook.has_parameter_list {
                    return Err(self.error(
                        format!("get hook of property {property} must not have a parameter list"),
                        line,
                    ));
                }
                &[]
            };
            let hook_name = format!("{property}::{}", if setter { "set" } else { "get" });
            let source = FunctionSource {
                span: phera_source::Span {
                    start: hook.name.span.start,
                    end: hook.span.end,
                },
                doc_comment: hook.doc_comment,
                attributes: &hook.attributes,
                name: &hook_name,
                by_reference: hook.by_reference,
                parameters,
                return_type: if setter { None } else { declaration.ty },
                body: FunctionBody::Statements(&[]),
            };
            let definition_line = self.lines.line(hook.name.span.start);
            let mut body = self.function_builder(&source, &hook_name, &[], definition_line)?;
            body.function.metadata.declaration = body.declaration_metadata(
                source.attributes,
                source.span,
                source.doc_comment,
                Some(declaration.class),
                AttributePlacement::Declaration(AttributeTarget::Method),
            )?;
            if parameters
                .iter()
                .any(|parameter| !parameter.attributes.is_empty())
            {
                body.function.metadata.parameters = parameters
                    .iter()
                    .map(|parameter| body.reflected_parameter(parameter))
                    .collect::<Result<_, _>>()?;
            }
            if body.has_builtin(source.attributes, "Deprecated")
                || body.has_builtin(source.attributes, "NoDiscard")
            {
                body.emit(Instruction::AttributeDiagnostics, line);
            }
            body.trait_scope = declaration.trait_scope.map(str::to_owned);
            body.hooked_property = Some(declaration.property.to_owned());
            body.function.return_type = if setter {
                Some("void".into())
            } else {
                declaration.ty.map(|ty| self.declaration_type(ty))
            };
            if setter && hook.by_reference {
                self.compile_diagnostics.borrow_mut().push(phera_source::CompileDiagnostic {
                    kind: phera_source::CompileDiagnosticKind::Deprecated,
                    message: format!("{property}::{name}(): Returning by reference from a void function is deprecated"),
                    line,
                });
            }
            body.compile_hook_body(&hook.body, setter, self.lines.line(hook.span.end))?;
            result.is_virtual &= !body.uses_hook_backing;
            let function = self.append_function_body(&mut body)?;
            *entry = Some(PropertyHook {
                function,
                is_abstract: abstract_hook,
                is_final: hook.is_final,
            });
        }
        if !declaration.has_parent {
            if result.is_virtual && declaration.default.is_some() {
                return Err(self.error(
                    format!("Cannot specify default value for virtual hooked property {property}"),
                    declaration.line,
                ));
            }
            if !result.is_virtual
                && result.set.is_some()
                && result.get.as_ref().is_some_and(|get| {
                    self.functions
                        .get(get.function)
                        .is_some_and(|function| function.returns_reference)
                })
            {
                return Err(self.error(format!("Get hook of backed property {}::{} with set hook may not return by reference", declaration.class, declaration.property), declaration.line));
            }
        }
        if declaration.modifiers.is_abstract
            && !result
                .get
                .iter()
                .chain(result.set.iter())
                .any(|hook| hook.is_abstract)
        {
            return Err(self.error(
                format!("Abstract property {property} must specify at least one abstract hook"),
                declaration.line,
            ));
        }
        Ok(Box::new(result))
    }

    fn validate_set_parameters(
        &self,
        hook: &ast::PropertyHook,
        property: &str,
        typed: bool,
        line: usize,
    ) -> Result<(), CompileError> {
        let name = &hook.name.name;
        let [parameter] = hook.parameters.as_slice() else {
            return Err(self.error(
                format!("{name} hook of property {property} must accept exactly one parameters"),
                line,
            ));
        };
        let parameter_name = &parameter.name;
        let invalid = if parameter.by_reference {
            Some("be pass-by-reference")
        } else if parameter.variadic {
            Some("be variadic")
        } else if parameter.default.is_some() {
            Some("have a default value")
        } else {
            None
        };
        if let Some(invalid) = invalid {
            return Err(self.error(
                format!("Parameter ${parameter_name} of {name} hook {property} must not {invalid}"),
                line,
            ));
        }
        if typed != parameter.ty.is_some() {
            return Err(self.error(format!("Type of parameter ${parameter_name} of hook {property}::set must be compatible with property type"), line));
        }
        Ok(())
    }
}
