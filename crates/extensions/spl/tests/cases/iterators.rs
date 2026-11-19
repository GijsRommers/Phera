//! Iterator adapters compare direct protocol methods and ordinary iterator consumers.

use super::{Cases, HOSTILE, methods, push};

pub(super) fn wrappers() -> Cases {
    let mut cases = Cases::new();
    for expression in [
        "new EmptyIterator",
        "new IteratorIterator($a)",
        "new NoRewindIterator($a)",
        "new LimitIterator($a,1,2)",
        "new CallbackFilterIterator($a, fn($n)=>$n%2)",
        "new InfiniteIterator($a)",
        "new CachingIterator($a,256)",
        "new AppendIterator",
        "new MultipleIterator",
        "new RegexIterator($a, '/[14]/')",
    ] {
        let mut setup = format!("$a = new ArrayIterator([1,2,3,4]); $o = {expression};");
        if expression == "new AppendIterator" {
            setup.push_str("$o->append($a); $o->append(new ArrayIterator([6]));");
        }
        if expression == "new MultipleIterator" {
            setup.push_str("$o->attachIterator($a); $o->attachIterator(new ArrayIterator([6,7]));");
        }
        methods(
            &mut cases,
            &setup,
            &[
                "$o->current()",
                "$o->key()",
                "$o->valid()",
                "[$o->rewind(), $o->valid()]",
                "[$o->rewind(),$o->next(),$o->current(),$o->key()]",
            ],
        );
    }
    let limit = "$o=new LimitIterator(new ArrayIterator([1,2,3,4]),1,2); $o->rewind();";
    for offset in HOSTILE {
        push(
            &mut cases,
            "limit-seek",
            limit,
            &format!("[$o->seek({offset}), $o->getPosition(), $o->current()]"),
        );
    }
    for pattern in ["''", "'bad'", "'/(/'", "'/b/'", "'/a/'", "[]", r#""\x00""#] {
        for mode in [0, 1, 2, 3, 4, 5] {
            push(
                &mut cases,
                "regex",
                "$a=new ArrayIterator(['ba','c','abc']);",
                &format!("iterator_to_array(new RegexIterator($a,{pattern},{mode}))"),
            );
        }
    }
    for flags in HOSTILE {
        push(
            &mut cases,
            "caching",
            "$a=new ArrayIterator([1,2]);",
            &format!("(new CachingIterator($a,{flags}))->getFlags()"),
        );
    }
    let cached = "$o=new CachingIterator(new ArrayIterator(['k'=>1]),256); $o->rewind();";
    for key in HOSTILE {
        for method in ["offsetGet", "offsetExists", "offsetUnset", "offsetSet"] {
            let extra = if method == "offsetSet" { ",3" } else { "" };
            push(
                &mut cases,
                method,
                cached,
                &format!("[$o->{method}({key}{extra}),$o->getCache()]"),
            );
        }
    }
    methods(
        &mut cases,
        cached,
        &[
            "$o->hasNext()",
            "$o->getFlags()",
            "$o->count()",
            "$o->__toString()",
            "[$o->setFlags(0),$o->getCache()]",
            "[$o->next(), $o->hasNext(), $o->getCache()]",
        ],
    );
    cases
}

pub(super) fn recursive() -> Cases {
    let mut cases = Cases::new();
    let uninitialized = concat!(
        "if (!class_exists('SkipsRecursiveConstructor')) { ",
        "class SkipsRecursiveConstructor extends RecursiveIteratorIterator ",
        "{ function __construct() {} } } $o = new SkipsRecursiveConstructor;"
    );
    methods(
        &mut cases,
        uninitialized,
        &[
            "$o->getDepth()",
            "$o->getInnerIterator()",
            "$o->getSubIterator()",
            "$o->getMaxDepth()",
            "$o->setMaxDepth(-2)",
            "$o->beginIteration()",
            "$o->rewind()",
            "$o->current()",
            "$o->valid()",
            "$o->next()",
            "$o->callHasChildren()",
            "$o->callGetChildren()",
        ],
    );
    let setup = "$a=new RecursiveArrayIterator(['x'=>['n'=>1,2],'y'=>3]);";
    for expression in [
        "new RecursiveCallbackFilterIterator($a, fn()=>true)",
        "new ParentIterator($a)",
        "new RecursiveRegexIterator($a, '/[12]/')",
        "new RecursiveCachingIterator($a, 16)",
    ] {
        methods(
            &mut cases,
            &format!("{setup} $o={expression};"),
            &[
                "[$o->rewind(), $o->key(), $o->current(), $o->hasChildren(), $o->getChildren()]",
                "iterator_to_array(new RecursiveIteratorIterator($o))",
            ],
        );
    }
    for mode in [-1, 0, 1, 2, 3] {
        let setup = format!("{setup} $o=new RecursiveIteratorIterator($a,{mode});");
        methods(
            &mut cases,
            &setup,
            &[
                "iterator_to_array($o)",
                "[$o->rewind(), $o->getDepth(), $o->current(), $o->key()]",
                "[$o->getMaxDepth(), $o->setMaxDepth(0), $o->getMaxDepth(), iterator_to_array($o)]",
                "[$o->rewind(), $o->callHasChildren(), $o->callGetChildren(), $o->getSubIterator(0)]",
            ],
        );
    }
    methods(
        &mut cases,
        &format!("{setup} $o=new RecursiveTreeIterator($a);"),
        &[
            "iterator_to_array($o)",
            "$o->getPostfix()",
            "[$o->rewind(), $o->getPrefix(), $o->current(), $o->getEntry()]",
            "[$o->setPrefixPart(2,'-'), $o->setPostfix('.'), iterator_to_array($o)]",
            "$o->setPrefixPart(-1,'x')",
        ],
    );
    cases
}

pub(super) fn protocols() -> Cases {
    let mut cases = Cases::new();
    for name in [
        "LogicException",
        "BadFunctionCallException",
        "BadMethodCallException",
        "DomainException",
        "InvalidArgumentException",
        "LengthException",
        "OutOfRangeException",
        "RuntimeException",
        "OutOfBoundsException",
        "OverflowException",
        "RangeException",
        "UnderflowException",
        "UnexpectedValueException",
    ] {
        push(
            &mut cases,
            name,
            "",
            &format!("get_parent_class(new {name})"),
        );
    }
    for name in [
        "SeekableIterator",
        "OuterIterator",
        "RecursiveIterator",
        "SplObserver",
        "SplSubject",
    ] {
        push(&mut cases, name, "", &format!("interface_exists('{name}')"));
    }
    methods(
        &mut cases,
        "",
        &[
            "extension_loaded('spl')",
            "in_array('SPL',get_loaded_extensions(),true)",
            "is_subclass_of('FilterIterator','Iterator')",
            "is_subclass_of('FilterIterator','OuterIterator')",
            "spl_classes()['SplObjectStorage']",
            "spl_classes()['RecursiveArrayIterator']",
            "spl_classes()['InvalidArgumentException']",
        ],
    );
    cases
}
