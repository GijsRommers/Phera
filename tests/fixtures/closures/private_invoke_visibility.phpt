--TEST--
Nonpublic invoke is callable and warns at declaration time
--FILE--
<?php
class A { private function __invoke() { return 3; } }
$f = new A;
var_dump(is_callable($f), $f());
--EXPECTF--
Warning: The magic method A::__invoke() must have public visibility in %s on line 2
bool(true)
int(3)
