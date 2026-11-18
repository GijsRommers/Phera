--TEST--
invoke closure from callable
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
$callback = Closure::fromCallable($object); echo $callback(4);
--EXPECT--
5
