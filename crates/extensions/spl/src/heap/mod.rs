//! A binary heap keeps the largest or smallest comparable item at the root.
//!
//! ```php
//! $heap = new SplMinHeap;
//! $heap->insert(2);
//! $heap->insert(1);
//! echo $heap->extract(); // 1
//! ```

mod operations;
mod registration;
mod sift;
mod views;

pub(crate) use registration::register;

use phera_extension_api::{ArrayKey, NativeError, Object, PhpArray, Value};

const HEAP: &str = "SplHeap";
const PRIORITY_QUEUE: &str = "SplPriorityQueue";

#[derive(Clone, Copy, Default)]
enum Extract {
    Heap,
    #[default]
    Data,
    Priority,
    Both,
}

impl Extract {
    fn from_flags(flags: i64) -> Option<Self> {
        match flags & 3 {
            1 => Some(Self::Data),
            2 => Some(Self::Priority),
            3 => Some(Self::Both),
            _ => None,
        }
    }

    fn flags(self) -> i64 {
        match self {
            Self::Heap => 0,
            Self::Data => 1,
            Self::Priority => 2,
            Self::Both => 3,
        }
    }

    fn project(self, entry: &Entry) -> Value {
        match self {
            Self::Heap | Self::Data => entry.data.deref().clone(),
            Self::Priority => entry.weight().deref().clone(),
            Self::Both => Value::from(
                [
                    (ArrayKey::str("data"), entry.data.clone()),
                    (ArrayKey::str("priority"), entry.weight().clone()),
                ]
                .into_iter()
                .collect::<PhpArray>(),
            ),
        }
    }
}

#[derive(Clone)]
struct Entry {
    data: Value,
    priority: Option<Value>,
}

impl Entry {
    fn weight(&self) -> &Value {
        self.priority.as_ref().unwrap_or(&self.data)
    }
}

#[derive(Clone, Copy, Default)]
enum Status {
    #[default]
    Ready,
    Modifying,
    Corrupted,
}

#[derive(Clone, Default)]
struct Heap {
    items: Vec<Entry>,
    extract: Extract,
    status: Status,
}

impl Heap {
    fn projection(&self, object: &Object) -> Extract {
        if object.class.is_instance_of(PRIORITY_QUEUE) {
            self.extract
        } else {
            Extract::Heap
        }
    }

    fn validate(&self, writing: bool) -> Result<(), NativeError> {
        match self.status {
            Status::Corrupted => Err(runtime(
                "Heap is corrupted, heap properties are no longer ensured.",
            )),
            Status::Modifying if writing => Err(runtime(
                "Heap cannot be changed when it is already being modified.",
            )),
            _ => Ok(()),
        }
    }
}

fn read<T>(object: &Object, f: impl FnOnce(&Heap) -> T) -> Result<T, NativeError> {
    crate::read_state(object, f)
}

fn update<T>(object: &Object, f: impl FnOnce(&mut Heap) -> T) -> Result<T, NativeError> {
    crate::update_state(object, f)
}

fn runtime(message: &str) -> NativeError {
    NativeError::throw("RuntimeException", message)
}

impl crate::native_state::ForkValues for Heap {
    fn fork_values(
        &mut self,
        fork: &mut phera_extension_api::value::Fork<'_>,
    ) -> Result<(), String> {
        for entry in &mut self.items {
            crate::native_state::fork_value(&mut entry.data, fork);
            if let Some(priority) = &mut entry.priority {
                crate::native_state::fork_value(priority, fork);
            }
        }
        Ok(())
    }
}

pub(crate) const FORK: crate::native_state::StoredFork = crate::native_state::fork_as::<Heap>;
