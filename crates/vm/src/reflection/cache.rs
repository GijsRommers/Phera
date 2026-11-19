//! Repeated `new ReflectionClass(User::class)` shares immutable declaration snapshots.

use std::{collections::HashMap, rc::Rc};

use phera_extension_api::NativeError;
use phera_extension_api::reflection::{
    ClassInfo, FunctionInfo, FunctionTarget, MemberKind, Members,
};

use crate::{
    Vm,
    classes::{Class, ClassIndex},
};

#[derive(Default)]
struct ReflectionCache {
    classes: HashMap<ClassIndex, ReflectedClass>,
    functions: HashMap<FunctionKey, Rc<FunctionInfo>>,
}

#[derive(Default)]
struct ReflectedClass {
    info: Option<Rc<ClassInfo>>,
    methods: Option<Members>,
    properties: Option<Members>,
    constants: Option<Members>,
}

#[derive(PartialEq, Eq, Hash)]
enum FunctionKey {
    Named(String),
    Method(String, String),
}

impl ReflectedClass {
    fn members(&mut self, kind: MemberKind) -> &mut Option<Members> {
        match kind {
            MemberKind::Method => &mut self.methods,
            MemberKind::Property => &mut self.properties,
            MemberKind::Constant => &mut self.constants,
        }
    }
}

impl Vm {
    fn reflected_class_cache(&mut self, index: ClassIndex) -> &mut ReflectedClass {
        self.state
            .get_or_default::<ReflectionCache>()
            .classes
            .entry(index)
            .or_default()
    }

    pub(super) fn cached_class_info(&mut self, class: &Class) -> Rc<ClassInfo> {
        if let Some(info) = &self.reflected_class_cache(class.index).info {
            return info.clone();
        }
        let info = Rc::new(self.reflection_class_info(class));
        self.reflected_class_cache(class.index).info = Some(info.clone());
        info
    }

    pub(super) fn cached_members(&mut self, class: &Class, kind: MemberKind) -> Members {
        if let Some(members) = self.reflected_class_cache(class.index).members(kind) {
            return members.clone();
        }
        let members = match kind {
            MemberKind::Method => self.reflection_methods(class),
            MemberKind::Property => self.reflection_properties(class),
            MemberKind::Constant => self.reflection_constants(class),
        };
        let members = Rc::new(members.into_iter().map(Rc::new).collect());
        *self.reflected_class_cache(class.index).members(kind) = Some(Rc::clone(&members));
        members
    }

    pub(super) fn cached_function(
        &mut self,
        target: &FunctionTarget,
    ) -> Result<Rc<FunctionInfo>, NativeError> {
        let key = match target {
            FunctionTarget::Named(name) => FunctionKey::Named(name.to_ascii_lowercase()),
            FunctionTarget::Method { class, name } => {
                FunctionKey::Method(class.to_ascii_lowercase(), name.to_ascii_lowercase())
            }
            FunctionTarget::Closure(_) | FunctionTarget::ClosureInvoke(_) => {
                return self.reflection_function(target).map(Rc::new);
            }
        };
        let cache = self.state.get_or_default::<ReflectionCache>();
        if let Some(info) = cache.functions.get(&key) {
            return Ok(info.clone());
        }
        let info = Rc::new(self.reflection_function(target)?);
        self.state
            .get_or_default::<ReflectionCache>()
            .functions
            .insert(key, info.clone());
        Ok(info)
    }
}
