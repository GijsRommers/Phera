//! In `class C { use T; }`, T's methods become ordinary methods on C.

use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use phera_bytecode::{ClassDeclaration, ClassKind, CompiledUnit, Visibility};
use phera_extension_api::NativeError;

use super::{Class, Method, MethodBody, inheritance_checks};
use crate::Vm;

type MethodTable = HashMap<String, Rc<Method>>;

impl Vm {
    pub(super) fn resolve_traits(
        &self,
        declaration: &ClassDeclaration,
    ) -> Result<Vec<Rc<Class>>, NativeError> {
        let mut traits = Vec::new();
        let mut seen = HashSet::new();
        for name in declaration.traits.iter().flat_map(|usage| &usage.names) {
            let Some(trait_class) = self.classes.get(name) else {
                return Err(Self::class_miss(name, "Trait"));
            };
            if trait_class.kind != ClassKind::Trait {
                let class = display(&declaration.name);
                return Err(NativeError::error(format!(
                    "{class} cannot use {name} - it is not a trait"
                )));
            }
            if seen.insert(trait_class.index) {
                traits.push(trait_class);
            }
        }
        Ok(traits)
    }

    pub(super) fn compose_trait_methods(
        &mut self,
        declaration: &ClassDeclaration,
        unit: &CompiledUnit,
        traits: &[Rc<Class>],
        methods: &mut MethodTable,
    ) -> Result<(), NativeError> {
        let class = &declaration.name;
        let rules = self.trait_method_rules(declaration, traits)?;
        let mut imports = Imports {
            class,
            own: methods,
            methods: Vec::new(),
            requirements: Vec::new(),
        };
        for provider in traits {
            let mut entries: Vec<_> = provider.methods.iter().collect();
            entries.sort_by_key(|(key, method)| {
                let order = match method.body {
                    MethodBody::User(function) => function.index(),
                    _ => 0,
                };
                (order, *key)
            });
            for (key, method) in entries {
                let provider_name = provider.name.to_ascii_lowercase();
                let mut visibility = None;
                let mut is_final = false;
                for alias in rules.aliases(&provider_name, key) {
                    if let Some(name) = &alias.name {
                        let mut linked =
                            copy_method(method, name, &provider.name, alias.visibility);
                        linked.is_final |= alias.is_final;
                        self.check_trait_finality(unit, declaration.line, method, &linked);
                        imports.add(self, linked, &method.name)?;
                    } else {
                        visibility = alias.visibility.or(visibility);
                        is_final |= alias.is_final;
                    }
                }
                if !rules.excludes(&provider_name, key) {
                    let mut linked = copy_method(method, &method.name, &provider.name, visibility);
                    linked.is_final |= is_final;
                    self.check_trait_finality(unit, declaration.line, method, &linked);
                    imports.add(self, linked, &method.name)?;
                }
            }
        }
        let Imports {
            methods: imported,
            requirements,
            ..
        } = imports;
        for (name, import) in imported {
            let linked = self.import_method(&import, class);
            if let Some(inherited) = methods.get(&name) {
                inheritance_checks::check_override(display(class), inherited, &linked)?;
                self.check_method_contract(inherited, &linked, unit)?;
            }
            methods.insert(name, linked);
        }
        for requirement in requirements {
            let key = requirement.name.to_ascii_lowercase();
            if let Some(implementation) = methods.get(&key) {
                if requirement.is_static != implementation.is_static {
                    let (before, after) = if requirement.is_static {
                        ("static", "non static")
                    } else {
                        ("non static", "static")
                    };
                    return Err(NativeError::error(format!(
                        "Cannot make {before} method {}::{}() {after} in class {}",
                        requirement.declaring,
                        requirement.name,
                        display(class),
                    )));
                }
                self.check_trait_contract(&requirement, implementation, class, unit)?;
            } else {
                let linked = self.import_method(&requirement, class);
                methods.insert(key, linked);
            }
        }
        Ok(())
    }

    fn check_trait_finality(
        &mut self,
        unit: &CompiledUnit,
        line: u32,
        original: &Method,
        imported: &Method,
    ) {
        if !original.is_final
            && imported.is_final
            && imported.visibility == Visibility::Private
            && !imported.name.eq_ignore_ascii_case("__construct")
        {
            self.warn_at_declaration(
                unit,
                line,
                "Private methods cannot be final as they are never overridden by other classes"
                    .into(),
            );
        }
    }

    fn trait_body_order(&self, method: &Method) -> usize {
        match method.body {
            MethodBody::User(function) => self.code.original_function(function).index(),
            _ => 0,
        }
    }

    fn import_method(&mut self, method: &Method, class: &str) -> Rc<Method> {
        let mut linked = copy_method(method, &method.name, class, None);
        if let MethodBody::User(source) = linked.body {
            let name = format!("{}::{}", display(class), method.name);
            linked.body = MethodBody::User(self.code.import_trait_method(
                source,
                self.classes.next_index(),
                name,
            ));
        }
        Rc::new(linked)
    }
}

struct Imports<'a> {
    class: &'a str,
    own: &'a MethodTable,
    methods: Vec<(String, Method)>,
    requirements: Vec<Method>,
}

impl Imports<'_> {
    fn add(&mut self, vm: &Vm, method: Method, original_name: &str) -> Result<(), NativeError> {
        let key = method.name.to_ascii_lowercase();
        if method.is_abstract {
            self.requirements.push(method);
            return Ok(());
        }
        if self
            .own
            .get(&key)
            .is_some_and(|owned| owned.declaring == self.class)
        {
            return Ok(());
        }
        if let Some((_, previous)) = self.methods.iter().find(|(name, _)| name == &key) {
            if vm.trait_body_order(previous) == vm.trait_body_order(&method)
                && previous.visibility == method.visibility
                && previous.is_final == method.is_final
            {
                return Ok(());
            }
            let class = display(self.class);
            return Err(NativeError::error(format!(
                "Trait method {}::{} has not been applied as {class}::{}, because of collision with {}::{}",
                method.declaring, original_name, method.name, previous.declaring, previous.name,
            )));
        }
        self.methods.push((key, method));
        Ok(())
    }
}

fn copy_method(
    method: &Method,
    name: &str,
    declaring: &str,
    visibility: Option<Visibility>,
) -> Method {
    Method {
        signature: std::cell::OnceCell::new(),
        name: name.into(),
        declaring: declaring.into(),
        visibility: visibility.unwrap_or(method.visibility),
        is_static: method.is_static,
        is_final: method.is_final,
        is_abstract: method.is_abstract,
        body: method.body.clone(),
    }
}

pub(super) fn display(class: &str) -> &str {
    class.split('\0').next().unwrap_or(class)
}

pub(crate) fn deprecated_direct_access(kind: &str, member: &str) -> NativeError {
    NativeError::throw(
        "Phera\\NotSupportedException",
        format!(
            "Direct access to static trait {kind} {member} is not supported by Phera: deprecated since PHP 8.1"
        ),
    )
}
