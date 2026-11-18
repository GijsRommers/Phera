--TEST--
invoke callable type
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
function run(callable $callback) { echo $callback(4); } run($object);
--EXPECT--
5
