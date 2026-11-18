--TEST--
invoke named argument
--FILE--
<?php
class Invokable { function __invoke(int $n) { return $n + 1; } } $object = new Invokable;
echo $object(n: 4);
--EXPECT--
5
