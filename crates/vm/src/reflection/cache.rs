//! Repeated `new ReflectionClass(User::class)` shares immutable declaration snapshots.

use std::rc::Rc;

use foldhash::HashMap;
use phera_extension_api::NativeError;
use phera_extension_api::reflection::{
    ClassInfo, FunctionInfo, FunctionTarget, MemberKind, Members, MethodHandle, PropertyHandle,
    PropertyHookKind,
};

use crate::{
    Vm,
    classes::{Class, ClassIndex},
};

#[derive(Default, Clone)]
pub(crate) struct ReflectionCache {
    classes: HashMap<ClassIndex, ReflectedClass>,
    functions: HashMap<FunctionKey, Rc<FunctionInfo>>,
}

#[derive(Default, Clone)]
struct ReflectedClass {
    info: Option<Rc<ClassInfo>>,
    methods: Option<Members>,
    /// The reflected function of each method in `methods`, by position,
    /// made when a `ReflectionMethod` of it is first made
    /// (`FunctionTarget::Member`).
    method_functions: Vec<Option<Rc<FunctionInfo>>>,
    properties: Option<Members>,
    constants: Option<Members>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum FunctionKey {
    Named(String),
    Method(u32, String),
    PropertyHook(PropertyHandle, PropertyHookKind),
}

impl ReflectedClass {
    fn members(&self, kind: MemberKind) -> Option<&Members> {
        match kind {
            MemberKind::Method => self.methods.as_ref(),
            MemberKind::Property => self.properties.as_ref(),
            MemberKind::Constant => self.constants.as_ref(),
        }
    }

    fn members_mut(&mut self, kind: MemberKind) -> &mut Option<Members> {
        match kind {
            MemberKind::Method => &mut self.methods,
            MemberKind::Property => &mut self.properties,
            MemberKind::Constant => &mut self.constants,
        }
    }
}

impl Vm {
    fn reflection_cache_mut(&mut self) -> &mut ReflectionCache {
        Rc::make_mut(self.reflection_cache.get_or_insert_with(Rc::default))
    }

    fn reflected_class_cache_mut(&mut self, index: ClassIndex) -> &mut ReflectedClass {
        self.reflection_cache_mut()
            .classes
            .entry(index)
            .or_default()
    }

    pub(super) fn cached_class_info(&mut self, class: &Class) -> Rc<ClassInfo> {
        if let Some(info) = self
            .reflection_cache
            .as_deref()
            .and_then(|cache| cache.classes.get(&class.index))
            .and_then(|entry| entry.info.as_ref())
        {
            return info.clone();
        }
        let info = Rc::new(self.reflection_class_info(class));
        self.reflected_class_cache_mut(class.index).info = Some(info.clone());
        info
    }

    pub(super) fn cached_members(&mut self, class: &Class, kind: MemberKind) -> Members {
        if let Some(members) = self
            .reflection_cache
            .as_deref()
            .and_then(|cache| cache.classes.get(&class.index))
            .and_then(|entry| entry.members(kind))
        {
            return members.clone();
        }
        let members = match kind {
            MemberKind::Method => self.reflection_methods(class),
            MemberKind::Property => self.reflection_properties(class),
            MemberKind::Constant => self.reflection_constants(class),
        };
        let members = Rc::new(members.into_iter().map(Rc::new).collect());
        *self
            .reflected_class_cache_mut(class.index)
            .members_mut(kind) = Some(Rc::clone(&members));
        members
    }

    pub(super) fn cached_function(
        &mut self,
        target: &FunctionTarget,
    ) -> Result<Rc<FunctionInfo>, NativeError> {
        let key = match target {
            FunctionTarget::Named(name) => FunctionKey::Named(name.to_ascii_lowercase()),
            FunctionTarget::Method { class, name } => {
                let class = self
                    .classes
                    .get(class)
                    .ok_or_else(|| super::exception(format!("Class \"{class}\" does not exist")))?;
                FunctionKey::Method(class.index.number(), name.to_ascii_lowercase())
            }
            FunctionTarget::LinkedMethod { class, name } => {
                FunctionKey::Method(*class, name.to_ascii_lowercase())
            }
            FunctionTarget::PropertyHook { property, kind } => {
                FunctionKey::PropertyHook(*property, *kind)
            }
            FunctionTarget::Member(handle) => return self.member_function(*handle),
            FunctionTarget::Closure(_) | FunctionTarget::ClosureInvoke(_) => {
                return self.reflection_function(target).map(Rc::new);
            }
        };
        if let Some(info) = self
            .reflection_cache
            .as_deref()
            .and_then(|cache| cache.functions.get(&key))
        {
            return Ok(info.clone());
        }
        let info = Rc::new(self.reflection_function(target)?);
        self.reflection_cache_mut()
            .functions
            .insert(key, info.clone());
        Ok(info)
    }

    /// The reflected function of a method of a class's member list, from
    /// the class's column of them (made the first time).
    fn member_function(&mut self, handle: MethodHandle) -> Result<Rc<FunctionInfo>, NativeError> {
        let class = self
            .classes
            .numbered(handle.class)
            .ok_or_else(|| internal_error!("reflected method class"))?;
        let index = handle.index as usize;
        if let Some(Some(info)) = self
            .reflection_cache
            .as_deref()
            .and_then(|cache| cache.classes.get(&class.index))
            .and_then(|entry| entry.method_functions.get(index))
        {
            return Ok(info.clone());
        }
        let target = self.member_target(handle)?;
        let info = Rc::new(self.reflection_function(&target)?);
        let functions = &mut self.reflected_class_cache_mut(class.index).method_functions;
        if functions.len() <= index {
            functions.resize(index + 1, None);
        }
        if let Some(slot) = functions.get_mut(index) {
            *slot = Some(info.clone());
        }
        Ok(info)
    }

    /// The same method by its class and name, for the paths that look it up.
    pub(super) fn member_target(
        &mut self,
        handle: MethodHandle,
    ) -> Result<FunctionTarget, NativeError> {
        let class = self
            .classes
            .numbered(handle.class)
            .ok_or_else(|| internal_error!("reflected method class"))?;
        let members = self.cached_members(&class, MemberKind::Method);
        let member = members
            .get(handle.index as usize)
            .ok_or_else(|| internal_error!("reflected method"))?;
        Ok(FunctionTarget::Method {
            class: member.lookup_class.clone(),
            name: member.name.clone(),
        })
    }
}
