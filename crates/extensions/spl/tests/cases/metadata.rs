//! Compare SPL reflection with PHP, including `new ReflectionClass(ArrayObject::class)`.

use phera_extension_api::testing::oracle::run_php;

use super::{Cases, push};

const CONSTRUCTOR_ARGUMENTS: &[(&str, &str)] = &[
    ("ArrayIterator", "[[10]]"),
    ("ArrayObject", "[[10]]"),
    ("CachingIterator", "[new ArrayIterator]"),
    (
        "CallbackFilterIterator",
        "[new ArrayIterator, fn () => true]",
    ),
    ("DirectoryIterator", "[__DIR__]"),
    ("FilesystemIterator", "[__DIR__]"),
    ("GlobIterator", "[__FILE__]"),
    ("InfiniteIterator", "[new ArrayIterator]"),
    ("IteratorIterator", "[new ArrayIterator]"),
    ("LimitIterator", "[new ArrayIterator]"),
    ("NoRewindIterator", "[new ArrayIterator]"),
    ("ParentIterator", "[new RecursiveArrayIterator]"),
    ("RecursiveArrayIterator", "[[10]]"),
    ("RecursiveCachingIterator", "[new RecursiveArrayIterator]"),
    (
        "RecursiveCallbackFilterIterator",
        "[new RecursiveArrayIterator, fn () => true]",
    ),
    ("RecursiveDirectoryIterator", "[__DIR__]"),
    ("RecursiveIteratorIterator", "[new RecursiveArrayIterator]"),
    (
        "RecursiveRegexIterator",
        "[new RecursiveArrayIterator, '/x/']",
    ),
    ("RecursiveTreeIterator", "[new RecursiveArrayIterator]"),
    ("RegexIterator", "[new ArrayIterator, '/x/']"),
    ("SplFileInfo", "[__FILE__]"),
    ("SplFileObject", "[__FILE__]"),
    ("SplFixedArray", "[2]"),
];

pub(super) fn cases() -> Cases {
    let classes = run_php(
        "spl-metadata-inventory",
        "<?php echo implode(\"\\n\", spl_classes());",
    )
    .expect("PHP 8.5 supplies the SPL class inventory");
    let mut cases = Vec::new();
    for class in classes
        .iter()
        .map(String::as_str)
        .chain(["WeakMap", "WeakReference"])
    {
        let setup = format!("$r = new ReflectionClass('{class}');");
        let target = if class == "ArrayObject" {
            "new ArrayObject".to_owned()
        } else {
            format!("'{class}'")
        };
        push(
            &mut cases,
            &format!("{class}.class_implements"),
            "",
            &format!("array_values(class_implements({target}))"),
        );
        for method in ["isIterable", "isInternal", "getInterfaceNames"] {
            push(
                &mut cases,
                &format!("{class}.{method}"),
                &setup,
                &format!("$r->{method}()"),
            );
        }
        let methods = format!(
            "{setup} $names = array_map(fn ($method) => $method->getName(), $r->getMethods()); \
             sort($names, SORT_STRING);"
        );
        push(
            &mut cases,
            &format!("{class}.getMethods"),
            &methods,
            "[count($names), $names]",
        );
        let arguments = CONSTRUCTOR_ARGUMENTS
            .iter()
            .find_map(|(name, args)| (*name == class).then_some(*args))
            .unwrap_or("[]");
        push(
            &mut cases,
            &format!("{class}.newInstanceArgs"),
            &setup,
            &format!("get_class($r->newInstanceArgs({arguments}))"),
        );
        push(
            &mut cases,
            &format!("{class}.getConstants"),
            &format!("{setup} $constants = $r->getConstants(); ksort($constants, SORT_STRING);"),
            "$constants",
        );
    }

    let setup = "$object = new ArrayObject(['stored' => 7]); $object->label = 'visible';
        $r = new ReflectionObject($object);";
    for (label, expression) in [
        ("getName", "$r->getName()"),
        ("getInterfaceNames", "$r->getInterfaceNames()"),
        ("hasDynamicProperty", "$r->hasProperty('label')"),
        ("hidesStoredOffset", "$r->hasProperty('stored')"),
        (
            "getProperties",
            "array_map(fn ($property) => $property->getName(), $r->getProperties())",
        ),
        (
            "getDynamicValue",
            "$r->getProperty('label')->getValue($object)",
        ),
    ] {
        push(
            &mut cases,
            &format!("ReflectionObject.ArrayObject.{label}"),
            setup,
            expression,
        );
    }
    cases
}
