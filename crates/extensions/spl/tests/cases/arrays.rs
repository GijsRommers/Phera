//! Array wrappers compare element types, flag handling and cursor methods.

use super::{Cases, HOSTILE, methods, push};

pub(super) fn container(class: &str) -> Cases {
    let mut cases = Cases::new();
    let setup = format!("$o = new {class}(['' => 3, 'a' => null, 0 => 9, 1 => 12]);");
    for method in ["offsetGet", "offsetSet", "offsetExists", "offsetUnset"] {
        for key in HOSTILE {
            let extra = if method == "offsetSet" { ", 22" } else { "" };
            push(
                &mut cases,
                method,
                &setup,
                &format!("[$o->{method}({key}{extra}), $o->getArrayCopy()]"),
            );
        }
    }
    for flags in HOSTILE {
        push(
            &mut cases,
            "setFlags",
            &setup,
            &format!("[$o->setFlags({flags}), $o->getFlags()]"),
        );
    }
    for value in HOSTILE {
        push(
            &mut cases,
            "constructor",
            "",
            &format!("(new {class}({value}))->getArrayCopy()"),
        );
    }
    let sorting = format!("$o = new {class}(['y' => 'img12', 'x' => 'img2', 0 => 'IMG9']);");
    for method in ["ksort", "asort"] {
        for flag in ["0", "1", "2", "3", "5", "6", "14", "-1", "null", "[]"] {
            push(
                &mut cases,
                method,
                &sorting,
                &format!("[$o->{method}({flag}), $o->getArrayCopy()]"),
            );
        }
    }
    for method in ["uasort", "uksort"] {
        for callback in [
            "fn($a, $b) => $b <=> $a",
            "'strcmp'",
            "null",
            "'no_such_callback'",
        ] {
            push(
                &mut cases,
                method,
                &sorting,
                &format!("[$o->{method}({callback}), $o->getArrayCopy()]"),
            );
        }
    }
    methods(
        &mut cases,
        &sorting,
        &[
            "[$o->append('foo'), $o->getArrayCopy(), count($o), $o->count()]",
            "[$o->natsort(), $o->getArrayCopy()]",
            "[$o->natcasesort(), $o->getArrayCopy()]",
            "$o->__serialize()",
            "$o->__debugInfo()",
            "[$o->__unserialize([2, [9], [], null]), $o->__serialize()]",
            "$o->__unserialize([])",
            "$o->__unserialize([0, null, []])",
            "$o->__unserialize([0, [], [], 'no_such_class'])",
            "$o->__unserialize([0, [], [], 'stdClass'])",
            "[(array)$o, json_encode($o), $o == clone $o]",
            "$o->serialize()",
            "$o->unserialize('invalid')",
        ],
    );
    if class == "ArrayObject" {
        for value in HOSTILE {
            push(
                &mut cases,
                "exchangeArray",
                &setup,
                &format!("[$o->exchangeArray({value}), $o->getArrayCopy()]"),
            );
        }
        for class in [
            "'arrayiterator'",
            "'stdClass'",
            "'missing'",
            "'RecursiveArrayIterator'",
            "[]",
        ] {
            push(
                &mut cases,
                "setIteratorClass",
                &setup,
                &format!(
                    "[$o->setIteratorClass({class}), $o->getIteratorClass(), get_class($o->getIterator())]"
                ),
            );
        }
    } else {
        for offset in HOSTILE {
            push(
                &mut cases,
                "seek",
                &setup,
                &format!("[$o->seek({offset}), $o->valid(), $o->key(), $o->current()]"),
            );
        }
        methods(
            &mut cases,
            &setup,
            &[
                "[$o->current(), $o->key(), $o->valid()]",
                "[$o->next(), $o->current(), $o->rewind(), $o->current()]",
                "[$o->seek(1), $o->current(), (clone $o)->current()]",
            ],
        );
    }
    cases
}

pub(super) fn recursive() -> Cases {
    let mut cases = Cases::new();
    for value in HOSTILE.iter().chain(&["['k'=>2]", "(object)['a'=>1]"]) {
        for flags in [0, 4] {
            push(
                &mut cases,
                "children",
                &format!("$r=new RecursiveArrayIterator([{value}], {flags});"),
                "[$r->hasChildren(), $r->getChildren()]",
            );
        }
    }
    cases
}
