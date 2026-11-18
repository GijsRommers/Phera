//! Backing storage can be an array, ordinary object properties or another SPL array.
//!
//! ```php
//! $a = new ArrayObject(['x' => 1]);
//! $i = $a->getIterator(); // reads and writes the same storage
//! ```

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::{ArrayKey, NativeError, Object, PhpArray, Value};

use super::{ITERATOR, OBJECT};

#[derive(Clone)]
pub(super) enum Storage {
    Array(Rc<PhpArray>),
    Properties(Rc<Object>),
    Alias(Rc<Object>),
    OwnProperties,
}

impl Default for Storage {
    fn default() -> Self {
        Self::Array(Rc::default())
    }
}

#[derive(Clone, Default)]
pub(super) struct State {
    pub storage: Storage,
    pub flags: Flags,
    pub position: Position,
    pub iterator_class: Option<String>,
    pub sorting: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Flags(i64);

impl Flags {
    pub fn new(raw: i64) -> Self {
        Self(raw & 0xffff)
    }

    pub fn raw(self) -> i64 {
        self.0
    }

    pub fn standard_properties(self) -> bool {
        self.0 & 1 != 0
    }

    pub fn array_as_properties(self) -> bool {
        self.0 & 2 != 0
    }

    pub fn child_arrays_only(self) -> bool {
        self.0 & 4 != 0
    }
}

#[derive(Clone, Copy, Default)]
pub(super) enum Position {
    #[default]
    Initial,
    Slot(usize),
    End,
}

pub(super) fn base(object: &Object) -> &'static str {
    if object.class.is_instance_of(ITERATOR) {
        ITERATOR
    } else {
        OBJECT
    }
}

pub(super) fn read<T>(object: &Object, read: impl FnOnce(&State) -> T) -> Result<T, NativeError> {
    let data = object.native.try_borrow().map_err(|_| crate::busy())?;
    Ok(
        match data
            .as_ref()
            .and_then(|state| state.downcast_ref::<State>())
        {
            Some(state) => read(state),
            None => read(&State::default()),
        },
    )
}

pub(super) fn update<T>(
    object: &Object,
    change: impl FnOnce(&mut State) -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    let mut data = object.native.try_borrow_mut().map_err(|_| crate::busy())?;
    let state = data
        .get_or_insert_with(|| Box::<State>::default())
        .downcast_mut::<State>()
        .ok_or_else(|| NativeError::error("Invalid SPL array state"))?;
    change(state)
}

pub(super) fn check_mutable(object: &Object) -> Result<(), NativeError> {
    if read(object, |state| state.sorting)? {
        Err(NativeError::error(
            "Modification of ArrayObject during sorting is prohibited",
        ))
    } else {
        Ok(())
    }
}

pub(super) enum Target<'a> {
    Local(&'a Object),
    Shared(Rc<Object>),
    Properties(Rc<Object>),
    OwnProperties(&'a Object),
}

impl<'a> Target<'a> {
    pub fn of(object: &'a Object) -> Result<Self, NativeError> {
        let mut entry = read(object, |state| match &state.storage {
            Storage::Array(_) => None,
            storage => Some(storage.clone()),
        })?;
        if entry.is_none() {
            return Ok(Self::Local(object));
        }
        let mut shared = None;
        let mut visited = HashSet::new();
        visited.insert(object.id);
        while let Some(storage) = entry {
            match storage {
                Storage::Array(_) => break,
                Storage::Properties(properties) => return Ok(Self::Properties(properties)),
                Storage::OwnProperties => {
                    return Ok(match shared {
                        Some(shared) => Self::Properties(shared),
                        None => Self::OwnProperties(object),
                    });
                }
                Storage::Alias(other) => {
                    if !visited.insert(other.id) {
                        return Err(NativeError::error(
                            "Iterators or objects referencing each other",
                        ));
                    }
                    entry = read(&other, |state| match &state.storage {
                        Storage::Array(_) => None,
                        storage => Some(storage.clone()),
                    })?;
                    shared = Some(other);
                }
            }
        }
        Ok(shared.map_or(Self::Local(object), Self::Shared))
    }

    fn owner(&self) -> &Object {
        match self {
            Self::Local(object) | Self::OwnProperties(object) => object,
            Self::Shared(object) | Self::Properties(object) => object,
        }
    }

    pub fn is_properties(&self) -> bool {
        matches!(self, Self::Properties(_) | Self::OwnProperties(_))
    }

    pub fn read<T>(&self, read_array: impl FnOnce(&PhpArray) -> T) -> Result<T, NativeError> {
        if self.is_properties() {
            return Ok(read_array(&self.owner().to_array()));
        }
        read(self.owner(), |state| match &state.storage {
            Storage::Array(array) => read_array(array),
            _ => read_array(&PhpArray::new()),
        })
    }

    pub fn copy(&self) -> Result<Rc<PhpArray>, NativeError> {
        if self.is_properties() {
            return Ok(Rc::new(self.owner().to_array()));
        }
        read(self.owner(), |state| match &state.storage {
            Storage::Array(array) => array.clone(),
            _ => Rc::default(),
        })
    }

    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut PhpArray) -> Result<T, NativeError>,
    ) -> Result<T, NativeError> {
        if self.is_properties() {
            return self.owner().update(change).ok_or_else(crate::busy)?;
        }
        check_mutable(self.owner())?;
        update(self.owner(), |state| {
            let Storage::Array(array) = &mut state.storage else {
                return Err(NativeError::error(
                    "SPL array storage changed during mutation",
                ));
            };
            change(Rc::make_mut(array))
        })
    }
}

pub(super) fn public(key: &ArrayKey) -> bool {
    !matches!(key, ArrayKey::String(key) if key.as_bytes().first() == Some(&0))
}

pub(super) fn value_at(array: &PhpArray, position: usize) -> Value {
    array
        .entry_at(position)
        .map_or(Value::Null, |(_, value)| value.deref().clone())
}
