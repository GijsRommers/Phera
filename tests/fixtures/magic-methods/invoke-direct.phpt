--TEST--
invoke direct
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
echo $object(4);
--EXPECT--
5
