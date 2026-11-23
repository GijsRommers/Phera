--TEST--
Unsetting an uninitialized typed property enables the getter
--FILE--
<?php
class Getter { public int $value; function __get($n) { return 7; } }
$o = new Getter;
unset($o->value);
echo $o->value;
--EXPECT--
7
