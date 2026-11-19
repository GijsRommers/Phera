--TEST--
Eval signature
--INI--
fatal_error_backtraces=1
--FILE--
<?php
class ParentClass { public function visit(int $n) {} }
function declare_child($n) {
    eval('class ChildClass extends ParentClass { public function visit(string $n) {} }');
}
function outer() { declare_child(3); }
outer();
--EXPECTF--
Fatal error: Declaration of ChildClass::visit(string $n) must be compatible with ParentClass::visit(int $n) in %seval-signature.php(4) : eval()'d code on line 1
Stack trace:
#0 %seval-signature.php(6): declare_child(3)
#1 %seval-signature.php(7): outer()
#2 {main}
