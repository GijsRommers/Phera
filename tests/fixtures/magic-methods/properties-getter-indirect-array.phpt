--TEST--
A by-value getter array cannot be modified indirectly
--FILE--
<?php
class Getter { function __get($p) { return ['key' => 2]; } }
$o = new Getter;
$o->x['key'] = 7;
echo $o->x['key'];
--EXPECTF--
Notice: Indirect modification of overloaded property Getter::$x has no effect in %s on line 4
2
