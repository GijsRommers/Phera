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
    "DirectoryIterator",
    "DomainException",
    "EmptyIterator",
    "FilesystemIterator",
    "FilterIterator",
    "GlobIterator",
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
    "RecursiveDirectoryIterator",
    "RecursiveFilterIterator",
    "RecursiveIterator",
    "RecursiveIteratorIterator",
    "RecursiveRegexIterator",
    "RecursiveTreeIterator",
    "RegexIterator",
    "RuntimeException",
    "SeekableIterator",
    "SplDoublyLinkedList",
    "SplFileInfo",
    "SplFileObject",
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
    "SplTempFileObject",
    "UnderflowException",
    "UnexpectedValueException",
];

fn classes_deprecated(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    context.deprecated(
        "Function spl_classes() is deprecated since 8.6, use ReflectionExtension::getClassNames() instead".into(),
    );
    classes(context, args)
}

fn classes(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        CLASSES
            .iter()
            .map(|&name| (ArrayKey::str(name), Value::from(name)))
            .collect::<PhpArray>(),
    ))
}

pub(crate) fn register(registry: &mut Registry) {
    let handler = if registry
        .level()
        .has(phera_extension_api::Change::SplClassesDeprecated)
    {
        classes_deprecated
    } else {
        classes
    };
    registry.function(FunctionEntry::sync("spl_classes", &[], handler));
}
