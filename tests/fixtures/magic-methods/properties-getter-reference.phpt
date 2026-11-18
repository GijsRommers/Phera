--TEST--
By-reference __get exposes the returned alias
--FILE--
<?php
class Getter { private $backing = 1; function &__get($p) { return $this->backing; } }
$o = new Getter;
$ref =& $o->x;
$ref = 7;
echo $o->x;
--EXPECT--
7
