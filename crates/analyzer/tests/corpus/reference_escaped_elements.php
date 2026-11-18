<?php
// Rule: references / foreach / destructuring; reference cells may change across calls.
class A { function run($required) {} }
class B { function run() {} }
function replaceFirst(&$items) { $items[0] = new B; }
function throughForeach() {
    $items = [new A];
    foreach ($items as &$value) {}
    $value = new A;
    replaceFirst($items);
    $value->run();
}
function throughList() {
    $items = [new A];
    [&$value] = $items;
    $value = new A;
    replaceFirst($items);
    $value->run();
}
function throughAssignment() {
    $items = [new A];
    $value =& $items[0];
    $value = new A;
    replaceFirst($items);
    $value->run();
}
throughForeach();
throughList();
throughAssignment();
