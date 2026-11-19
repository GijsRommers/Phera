//! A linked list supports both FIFO and LIFO traversal of a deque.
//!
//! ```php
//! $stack = new SplStack;
//! $stack->push(1);
//! $stack->push(2);
//! echo $stack->pop(); // 2
//! ```

mod cursor;
mod dimensions;
mod methods;
mod registration;

pub(crate) use registration::register;

use std::collections::VecDeque;

use phera_extension_api::{NativeError, Object, Value};

const CLASS: &str = "SplDoublyLinkedList";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    Fifo,
    Lifo,
}

impl Order {
    fn reversed(self) -> Self {
        match self {
            Self::Fifo => Self::Lifo,
            Self::Lifo => Self::Fifo,
        }
    }
}

#[derive(Clone, Copy)]
enum Consumption {
    Keep,
    Delete,
}

#[derive(Clone, Copy)]
struct Mode {
    order: Order,
    consumption: Consumption,
    extra_flags: i64,
}

impl Mode {
    fn for_class(object: &Object) -> Self {
        let flags = if object.class.is_instance_of("SplStack") {
            6
        } else if object.class.is_instance_of("SplQueue") {
            4
        } else {
            0
        };
        Self::parse(flags)
    }

    fn parse(flags: i64) -> Self {
        Self {
            order: if flags & 2 == 0 {
                Order::Fifo
            } else {
                Order::Lifo
            },
            consumption: if flags & 1 == 0 {
                Consumption::Keep
            } else {
                Consumption::Delete
            },
            extra_flags: flags & !3,
        }
    }

    fn raw(self) -> i64 {
        let order = if self.order == Order::Lifo { 2 } else { 0 };
        let consumption = match self.consumption {
            Consumption::Keep => 0,
            Consumption::Delete => 1,
        };
        self.extra_flags | order | consumption
    }
}

#[derive(Clone)]
struct Entry {
    identity: u64,
    value: Value,
}

#[derive(Clone, Copy, Default)]
struct Position {
    entry: Option<(usize, u64)>,
    key: i64,
}

#[derive(Clone, Default)]
struct List {
    entries: VecDeque<Entry>,
    serial: u64,
    mode: Option<Mode>,
    cursor: Position,
}

impl List {
    fn entry(&mut self, value: Value) -> Entry {
        self.serial = self.serial.wrapping_add(1);
        Entry {
            identity: self.serial,
            value,
        }
    }

    fn slot(&self, index: usize, order: Order) -> Option<usize> {
        match order {
            Order::Fifo => (index < self.entries.len()).then_some(index),
            Order::Lifo => self.entries.len().checked_sub(index.checked_add(1)?),
        }
    }

    fn mode(&self, object: &Object) -> Mode {
        self.mode.unwrap_or_else(|| Mode::for_class(object))
    }
}

fn read<T>(object: &Object, f: impl FnOnce(&List) -> T) -> Result<T, NativeError> {
    crate::read_state(object, f)
}

fn update<T>(object: &Object, f: impl FnOnce(&mut List) -> T) -> Result<T, NativeError> {
    crate::update_state(object, f)
}

fn range(method: &str) -> NativeError {
    NativeError::throw(
        "OutOfRangeException",
        format!("{CLASS}::{method}(): Argument #1 ($index) is out of range"),
    )
}

impl crate::native_state::ForkValues for List {
    fn fork_values(
        &mut self,
        fork: &mut phera_extension_api::value::Fork<'_>,
    ) -> Result<(), String> {
        for entry in &mut self.entries {
            crate::native_state::fork_value(&mut entry.value, fork);
        }
        Ok(())
    }
}

pub(crate) const FORK: crate::native_state::StoredFork = crate::native_state::fork_as::<List>;
