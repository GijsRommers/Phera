--TEST--
invoke is callable
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
var_dump(is_callable($object));
--EXPECT--
bool(true)
