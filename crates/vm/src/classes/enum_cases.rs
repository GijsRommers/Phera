//! `Color::Red === Color::Red` because the request owns one object per case.
//!
//! ```php
//! enum Suit: string { case Hearts = 'H'; case Spades = 'S'; }
//! $a = Suit::Hearts;          // first read: makes the cases, checks 'H' != 'S'
//! $b = Suit::Hearts;          // later reads: the object from the case's slot
//! var_dump($a === $b);        // true
//! ```
//!
//! A case is made on its first use. Once it may be handed out without any
//! further work it is stored in its constant's slot of the request (see
//! `ClassStates`), where the interpreter reads it like `Post::LIMIT`. A unit
//! enum's case is stored when it is made. A backed enum's cases are stored
//! together when all of them exist and their backing values were found to
//! be distinct, so a read before that still runs the check.

use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use phera_bytecode::EnumBacking;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, Value};

use super::{
    Class, ClassIndex,
    class::{ConstantInitializer, ConstantState},
};
use crate::{
    Vm,
    initializer_tasks::{CaseLookup, Continuation, InitializerTask, Progress},
};

type BackingTable = HashMap<ArrayKey, Rc<Object>>;

#[derive(Default)]
pub(crate) struct EnumCases {
    objects: HashMap<ClassIndex, HashMap<String, Rc<Object>>>,
    initializing: HashSet<(ClassIndex, String)>,
    backed: HashMap<ClassIndex, BackingTable>,
    validating: HashSet<ClassIndex>,
}

/// The request stores one object per case and indexes backing values separately.
pub(crate) struct BackingBuilder {
    pub(crate) class: Rc<Class>,
    backing: EnumBacking,
    next: usize,
    table: BackingTable,
}

impl Vm {
    pub(crate) fn start_enum_case(&mut self, lookup: CaseLookup) -> Result<Progress, NativeError> {
        let CaseLookup {
            class,
            name,
            initializer,
            spelling,
        } = lookup;
        if let Some(object) = self
            .enum_cases
            .objects
            .get(&class.index)
            .and_then(|cases| cases.get(&name))
        {
            return Ok(Progress::Done(Value::Object(object.clone())));
        }
        let Some(function) = initializer else {
            return Ok(Progress::Done(self.finish_enum_case(class, name, None)));
        };
        if !self
            .enum_cases
            .initializing
            .insert((class.index, name.clone()))
        {
            let receiver = spelling.as_deref().unwrap_or(&class.name);
            return Err(NativeError::error(format!(
                "Cannot declare self-referencing constant {receiver}::{name}"
            )));
        }
        Ok(Progress::Await(
            InitializerTask::Expression(function, class.name.clone()),
            Continuation::Case(class, name),
        ))
    }

    pub(crate) fn finish_enum_case(
        &mut self,
        class: Rc<Class>,
        name: String,
        backing: Option<Value>,
    ) -> Value {
        self.cancel_enum_case(&class, name.clone());
        let object = self.classes.allocate(class.clone());
        object.set(ArrayKey::str("name"), Value::from(name.as_str()));
        if let Some(value) = backing {
            object.set(ArrayKey::str("value"), value);
        }
        let is_unit_enum = class
            .enum_info
            .as_ref()
            .is_some_and(|info| info.backing.is_none());
        if is_unit_enum {
            self.publish_enum_case(&class, &name, &object);
        }
        self.enum_cases
            .objects
            .entry(class.index)
            .or_default()
            .insert(name, object.clone());
        Value::Object(object)
    }

    /// From here on `Suit::Hearts` is read like any evaluated constant:
    /// the case object is in the constant's slot of this request.
    fn publish_enum_case(&mut self, class: &Class, name: &str, object: &Rc<Object>) {
        if let Some(constant) = class.constants.get(name)
            && let ConstantInitializer::EnumCase {
                slot: Some(slot), ..
            } = &constant.initializer
        {
            let case = Value::Object(object.clone());
            self.class_states
                .set_constant(*slot, ConstantState::Ready(case));
        }
    }

    /// A backed enum's cases become directly readable together, once every
    /// backing value was evaluated and no two are equal. Until then each
    /// read goes through the lookup, which starts that check.
    fn publish_backed_cases(&mut self, class: &Class) {
        let Some(cases) = self.enum_cases.objects.get(&class.index) else {
            return;
        };
        for (name, object) in cases {
            if let Some(constant) = class.constants.get(name)
                && let ConstantInitializer::EnumCase {
                    slot: Some(slot), ..
                } = &constant.initializer
            {
                let case = Value::Object(object.clone());
                self.class_states
                    .set_constant(*slot, ConstantState::Ready(case));
            }
        }
    }

    pub(crate) fn cancel_enum_case(&mut self, class: &Class, name: String) {
        self.enum_cases.initializing.remove(&(class.index, name));
    }

    pub(crate) fn needs_enum_backing(&self, class: &Class) -> bool {
        class
            .enum_info
            .as_ref()
            .is_some_and(|info| info.backing.is_some())
            && !self.enum_cases.backed.contains_key(&class.index)
            && !self.enum_cases.validating.contains(&class.index)
    }

    pub(crate) fn start_enum_backing(&mut self, class: Rc<Class>) -> Result<Progress, NativeError> {
        if !self.needs_enum_backing(&class) {
            return Ok(Progress::Done(Value::Null));
        }
        let info = class
            .enum_info
            .as_ref()
            .ok_or_else(|| internal_error!("enum backing metadata"))?;
        let backing = info
            .backing
            .ok_or_else(|| internal_error!("enum backing type"))?;
        let table = HashMap::with_capacity(info.names.len());
        self.enum_cases.validating.insert(class.index);
        self.next_enum_backing(BackingBuilder {
            class,
            backing,
            next: 0,
            table,
        })
    }

    pub(crate) fn resume_enum_backing(
        &mut self,
        mut builder: BackingBuilder,
        value: Value,
    ) -> Result<Progress, NativeError> {
        if let Err(error) = builder.add(value) {
            self.cancel_enum_backing(&builder.class);
            return Err(error);
        }
        builder.next += 1;
        self.next_enum_backing(builder)
    }

    fn next_enum_backing(&mut self, builder: BackingBuilder) -> Result<Progress, NativeError> {
        let name = builder
            .class
            .enum_info
            .as_ref()
            .and_then(|info| info.names.get(builder.next));
        if let Some(name) = name {
            return match case_lookup(&builder.class, name) {
                Ok(lookup) => Ok(Progress::Await(
                    InitializerTask::Case(lookup),
                    Continuation::Backing(builder),
                )),
                Err(error) => {
                    self.cancel_enum_backing(&builder.class);
                    Err(error)
                }
            };
        }
        self.cancel_enum_backing(&builder.class);
        self.publish_backed_cases(&builder.class);
        self.enum_cases
            .backed
            .insert(builder.class.index, builder.table);
        Ok(Progress::Done(Value::Null))
    }

    pub(crate) fn cancel_enum_backing(&mut self, class: &Class) {
        self.enum_cases.validating.remove(&class.index);
    }
}

impl BackingBuilder {
    fn add(&mut self, value: Value) -> Result<(), NativeError> {
        let Value::Object(object) = value else {
            return Err(internal_error!("backed enum object"));
        };
        let backing = object
            .get(&ArrayKey::str("value"))
            .ok_or_else(|| internal_error!("backed enum value"))?;
        let key = match (self.backing, backing) {
            (EnumBacking::Int, Value::Int(number)) => ArrayKey::Int(number),
            (EnumBacking::String, Value::String(string)) => ArrayKey::String(string),
            (backing, value) => {
                let expected = match backing {
                    EnumBacking::Int => "int",
                    EnumBacking::String => "string",
                };
                return Err(NativeError::type_error(format!(
                    "Enum case type {} does not match enum backing type {expected}",
                    value.type_name(),
                )));
            }
        };
        if let Some(previous) = self.table.get(&key) {
            let first = name_of(previous)?;
            let second = name_of(&object)?;
            return Err(NativeError::error(format!(
                "Duplicate value in enum {} for cases {first} and {second}",
                self.class.name
            )));
        }
        self.table.insert(key, object);
        Ok(())
    }
}

fn name_of(object: &Object) -> Result<String, NativeError> {
    if let Some(name) = object
        .get(&ArrayKey::interned_str(b"name"))
        .and_then(|value| match value {
            Value::String(name) => Some(name.to_string_lossy()),
            _ => None,
        })
    {
        Ok(name)
    } else {
        Err(internal_error!("enum case name"))
    }
}

pub(super) fn case_lookup(class: &Rc<Class>, name: &str) -> Result<CaseLookup, NativeError> {
    let Some(constant) = class.constants.get(name) else {
        return Err(internal_error!("enum case constant"));
    };
    let ConstantInitializer::EnumCase { backing, .. } = constant.initializer else {
        return Err(internal_error!("enum case constant initializer"));
    };
    Ok(CaseLookup {
        class: class.clone(),
        name: name.to_owned(),
        initializer: backing,
        spelling: None,
    })
}

impl Vm {
    pub(super) fn backed_enum_case(&self, class: &Class, key: &ArrayKey) -> Option<Value> {
        let object = self.enum_cases.backed.get(&class.index)?.get(key)?;
        Some(Value::Object(object.clone()))
    }
}

/// The fork's case objects are the copies of the snapshot's, so that
/// `Suit::Hearts === $card->suit` holds in the fork as it did before.
impl crate::snapshot::ForkCopy for EnumCases {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        EnumCases {
            objects: self.objects.fork_copy(fork),
            initializing: Default::default(),
            backed: self.backed.fork_copy(fork),
            validating: Default::default(),
        }
    }
}
