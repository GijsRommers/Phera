--TEST--
Unset permits a typed property to call the getter
--FILE--
<?php
class Getter { public int $value = 1; function __get($n) { return "7"; } }
$o = new Getter;
unset($o->value);
var_dump($o->value);
--EXPECT--
int(7)
