--TEST--
object-ids
--FILE--
<?php
class A { public function hi() { return 1; } public static function make(): static { return new static; } }
function id() { return spl_object_id(new A); }
echo "start ", id(), "\n";
foreach ([fn() => 1, fn() => 2, fn() => 3] as $f) { $f(); }
echo "after foreach over closures ", id(), "\n";
try { (new A)->nope(); } catch (Error $e) {} unset($e);
echo "after failed call on temporary ", id(), "\n";
var_dump(get_class(A::make()), get_class(A::make()));
echo "after temporaries as arguments ", id(), "\n";
function f(A $a) { return 1; } f(new A); f(new A);
echo "after passing temporaries ", id(), "\n";
$c = function () { return new A; }; $c(); $c();
echo "after closure results ", id(), "\n";
--EXPECT--
start 1
after foreach over closures 2
after failed call on temporary 1
string(1) "A"
string(1) "A"
after temporaries as arguments 1
after passing temporaries 1
after closure results 2
