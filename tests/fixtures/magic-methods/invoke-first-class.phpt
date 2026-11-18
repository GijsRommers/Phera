--TEST--
invoke first class
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
$callback = $object(...); echo $callback(4);
--EXPECT--
5
