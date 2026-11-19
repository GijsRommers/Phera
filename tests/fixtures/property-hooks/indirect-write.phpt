--TEST--
A getter returning an array by value disallows an indirect write
--FILE--
<?php
class C {
    public array $items = [] { get => $this->items; }
}
$c = new C;
try { $c->items[] = 1; } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Indirect modification of C::$items is not allowed
