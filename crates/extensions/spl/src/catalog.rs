//! `spl_classes()` returns the classes and interfaces that ship with SPL.
//!
//! ```php
//! spl_classes()['ArrayObject']; // 'ArrayObject'
//! ```

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, PhpArray, Registry, Value,
};

const CLASSES: &[&str] = &[
    "AppendIterator",
    "ArrayIterator",
    "ArrayObject",
    "BadFunctionCallException",
    "BadMethodCallException",
    "CachingIterator",
    "CallbackFilterIterator",
    "DomainException",
    "EmptyIterator",
    "FilterIterator",
    "InfiniteIterator",
    "InvalidArgumentException",
    "IteratorIterator",
    "LengthException",
    "LimitIterator",
    "LogicException",
    "MultipleIterator",
    "NoRewindIterator",
    "OuterIterator",
    "OutOfBoundsException",
    "OutOfRangeException",
    "OverflowException",
    "ParentIterator",
    "RangeException",
    "RecursiveArrayIterator",
    "RecursiveCachingIterator",
    "RecursiveCallbackFilterIterator",
    "RecursiveFilterIterator",
    "RecursiveIterator",
    "RecursiveIteratorIterator",
    "RecursiveRegexIterator",
    "RecursiveTreeIterator",
    "RegexIterator",
    "RuntimeException",
    "SeekableIterator",
    "SplDoublyLinkedList",
    "SplFixedArray",
    "SplHeap",
    "SplMaxHeap",
    "SplMinHeap",
    "SplObjectStorage",
    "SplObserver",
    "SplPriorityQueue",
    "SplQueue",
    "SplStack",
    "SplSubject",
    "UnderflowException",
    "UnexpectedValueException",
];

fn classes(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        CLASSES
            .iter()
            .map(|&name| (ArrayKey::str(name), Value::from(name)))
            .collect::<PhpArray>(),
    ))
}

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("spl_classes", &[], classes));
}
