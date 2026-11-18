//! Native storage, list, fixed and heap methods use one table per concrete receiver.

use super::{Cases, HOSTILE, methods, push};

pub(super) fn storage() -> Cases {
    let mut cases = Cases::new();
    let setup = "$o = new SplObjectStorage; $x = new stdClass; $o[$x] = 'info';";
    for method in [
        "attach",
        "detach",
        "contains",
        "offsetSet",
        "offsetUnset",
        "offsetExists",
        "offsetGet",
        "getHash",
        "seek",
        "count",
        "addAll",
        "removeAll",
        "removeAllExcept",
    ] {
        for value in HOSTILE.iter().chain(&["$x", "new SplObjectStorage"]) {
            let expression = if method == "getHash" {
                format!("strlen($o->{method}({value}))")
            } else {
                format!("[$o->{method}({value}), count($o)]")
            };
            push(&mut cases, method, setup, &expression);
        }
    }
    let more = format!(
        "{setup} $n = new SplObjectStorage; $n[$x] = 3; $y = new stdClass; $n[$y] = [1, [2]];"
    );
    methods(
        &mut cases,
        &more,
        &[
            "[$o->addAll($n), $o[$x], $o[$y], $o->count(COUNT_RECURSIVE)]",
            "[$o->removeAll($n), count($o)]",
            "[$o->removeAllExcept($n), count($o)]",
            "[$o->getInfo(), $o->setInfo(null), $o->getInfo()]",
            "[$o->current(), $o->key(), $o->next(), $o->valid(), $o->rewind(), $o->valid()]",
            "$o->__serialize()",
            "$o->__debugInfo()",
            "[$o->__unserialize($n->__serialize()), $o->__serialize()]",
            "$o->__unserialize([])",
            "$o->__unserialize([[1], []])",
            "$o->__unserialize([[3, null], []])",
            "$o->getHash($x) === spl_object_hash($x)",
            "$o == clone $o",
            "strlen($o->serialize()) > 0",
            "$o->unserialize('invalid')",
        ],
    );
    cases
}

pub(super) fn weak_reference() -> Cases {
    let mut cases = Cases::new();
    for value in HOSTILE {
        push(
            &mut cases,
            "create",
            "",
            &format!("WeakReference::create({value})->get()"),
        );
    }
    methods(
        &mut cases,
        "$x = new stdClass; $r = WeakReference::create($x); $s = WeakReference::create($x);",
        &[
            "$r === $s",
            "$r->get() === $x",
            "new WeakReference",
            "clone $r",
        ],
    );
    methods(
        &mut cases,
        "$x = new stdClass; $r = WeakReference::create($x); unset($x);",
        &["$r->get()"],
    );
    cases
}

pub(super) fn weak_map() -> Cases {
    let mut cases = Cases::new();
    let setup = "$x = new stdClass; $o = new WeakMap; $o[$x] = 1;";
    for method in ["offsetGet", "offsetSet", "offsetExists", "offsetUnset"] {
        for value in HOSTILE.iter().chain(&["$x"]) {
            let extra = if method == "offsetSet" { ", 2" } else { "" };
            if method == "offsetGet" && *value == "new stdClass" {
                push(&mut cases, method, setup, "$o->offsetGet(new stdClass)");
                // This diagnostic names the same handle returned by spl_object_id().
                cases.push((
                    "offsetGet_missing_object".to_owned(),
                    format!("{setup} $missing = new stdClass;"),
                    "(function () use ($o, $missing) { try { return $o->offsetGet($missing); } \
                     catch (Error $e) { \
                         $message = 'Object stdClass#' . spl_object_id($missing) . ' not contained in WeakMap'; \
                         if ($e->getMessage() !== $message) throw $e; \
                         return get_class($e) . ': exact diagnostic with the key handle'; \
                     } })()"
                        .to_owned(),
                ));
            } else {
                let expression = format!("[$o->{method}({value}{extra}), count($o)]");
                push(&mut cases, method, setup, &expression);
            }
        }
    }
    methods(
        &mut cases,
        setup,
        &[
            "count($o)",
            "$o->count()",
            "get_class($o->getIterator())",
            "$o == clone $o",
            "[$o->offsetSet($x, null), $o->offsetExists($x), isset($o[$x])]",
        ],
    );
    methods(
        &mut cases,
        &format!("{setup} unset($x);"),
        &["count($o)", "$o->count()"],
    );
    cases
}

pub(super) fn linked(class: &str) -> Cases {
    let mut cases = Cases::new();
    let setup = format!("$o = new {class}; $o->push('a'); $o->push('b');");
    for method in [
        "add",
        "offsetGet",
        "offsetSet",
        "offsetExists",
        "offsetUnset",
        "setIteratorMode",
    ] {
        for value in HOSTILE {
            let extra = if matches!(method, "add" | "offsetSet") {
                ", 22"
            } else {
                ""
            };
            push(
                &mut cases,
                method,
                &setup,
                &format!(
                    "[$o->{method}({value}{extra}), $o->__serialize(), $o->getIteratorMode()]"
                ),
            );
        }
    }
    methods(
        &mut cases,
        &setup,
        &[
            "[$o->rewind(), $o->current(), $o->next(), $o->key(), $o->prev(), $o->valid()]",
            "[$o->unshift(0), $o->bottom(), $o->top(), $o->pop(), $o->shift(), $o->count()]",
            "[$o->isEmpty(), $o->__debugInfo(), $o->__serialize(), (array)$o, json_encode($o)]",
            "[$o->__unserialize([2, [9], []]), $o->__serialize()]",
            "$o->serialize()",
            "$o->unserialize('data')",
            "$o->__unserialize([])",
        ],
    );
    if class == "SplQueue" {
        methods(
            &mut cases,
            &setup,
            &["[$o->enqueue(3), $o->dequeue(), count($o)]"],
        );
    }
    methods(
        &mut cases,
        &format!("$o = new {class};"),
        &[
            "$o->pop()",
            "$o->shift()",
            "$o->top()",
            "$o->bottom()",
            "$o->current()",
            "$o->key()",
            "$o->valid()",
            "$o->isEmpty()",
        ],
    );
    cases
}

pub(super) fn fixed() -> Cases {
    let mut cases = Cases::new();
    let setup = "$o = new SplFixedArray(2); $o[1] = 'b';";
    for method in [
        "offsetGet",
        "offsetSet",
        "offsetExists",
        "offsetUnset",
        "setSize",
    ] {
        for value in HOSTILE {
            // PHP terminates the process on an allocation overflow.
            if method == "setSize" && matches!(*value, "PHP_INT_MAX") {
                continue;
            }
            let extra = if method == "offsetSet" { ", 22" } else { "" };
            push(
                &mut cases,
                method,
                setup,
                &format!("[$o->{method}({value}{extra}), $o->toArray()]"),
            );
        }
    }
    for values in [
        "[]",
        "['a'=>2]",
        "[3=>4]",
        "[-2=>4]",
        "[PHP_INT_MAX=>1]",
        "null",
        "5",
    ] {
        for preserve in ["true", "false"] {
            push(
                &mut cases,
                "fromArray",
                "",
                &format!("SplFixedArray::fromArray({values},{preserve})"),
            );
        }
    }
    methods(
        &mut cases,
        setup,
        &[
            "[$o->getSize(), count($o), $o->count(), $o->jsonSerialize(), json_encode($o)]",
            "[$o->getIterator()->current(), $o->__serialize(), $o == clone $o]",
            "[(new SplFixedArray)->__unserialize([2,3]), $o->__serialize()]",
        ],
    );
    cases
}

pub(super) fn heap(class: &str) -> Cases {
    let mut cases = Cases::new();
    let plain = format!("$o = new {class};");
    let queued = class == "SplPriorityQueue";
    for value in HOSTILE {
        let row = if queued {
            format!("$o->insert('a', {value})")
        } else {
            format!("$o->insert({value})")
        };
        push(
            &mut cases,
            "insert",
            &plain,
            &format!("[{row}, $o->top(), $o->extract()]"),
        );
    }
    methods(
        &mut cases,
        &plain,
        &[
            "$o->top()",
            "$o->extract()",
            "$o->current()",
            "$o->key()",
            "$o->valid()",
            "$o->next()",
            "$o->isEmpty()",
            "$o->recoverFromCorruption()",
            "$o->isCorrupted()",
            "$o->__debugInfo()",
            "$o->__serialize()",
            "$o->__unserialize([])",
        ],
    );
    if queued {
        let setup = "$o=new SplPriorityQueue; $o->insert('a',3); $o->insert('b',2);";
        for flag in HOSTILE {
            push(
                &mut cases,
                "setExtractFlags",
                setup,
                &format!(
                    "[$o->setExtractFlags({flag}), $o->getExtractFlags(), $o->top(), $o->current()]"
                ),
            );
        }
        for priority in HOSTILE {
            push(
                &mut cases,
                "compare",
                setup,
                &format!("$o->compare(2,{priority})"),
            );
        }
    }
    cases
}
