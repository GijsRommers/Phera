//! ArrayObject creates live ArrayIterator views of its backing storage.
//!
//! ```php
//! $items = new ArrayObject([1]);
//! $items->getIterator()->append(2);
//! var_dump($items->getArrayCopy()); // [1, 2]
//! ```

mod cursor;
mod dimensions;
mod methods;
mod properties;
mod registration;
mod sorting;
mod state;

pub(crate) use registration::register;

const OBJECT: &str = "ArrayObject";
const ITERATOR: &str = "ArrayIterator";
const RECURSIVE: &str = "RecursiveArrayIterator";
