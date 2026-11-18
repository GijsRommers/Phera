--TEST--
An indirect array write can follow a getter reference
--FILE--
<?php
class Getter { private $backing = []; function &__get($p) { return $this->backing; } }
$o = new Getter;
$o->x['key'] = 7;
echo $o->x['key'];
--EXPECT--
7
