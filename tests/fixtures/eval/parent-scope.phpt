--TEST--
Parent scope
--FILE--
<?php
class A { const VALUE=5; } class B extends A { function f() { return eval('return parent::VALUE;'); } } var_dump((new B)->f());
--EXPECT--
int(5)
