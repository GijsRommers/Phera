--TEST--
Cloning retains references to nested objects
--FILE--
<?php
class Copy { public $child; }
$o = new Copy;
$o->child = (object)['value' => 1];
$c = clone $o;
$c->child->value = 2;
echo $o->child->value;
--EXPECT--
2
