--TEST--
Cloning a property whose last alias is its own slot separates the copy
--FILE--
<?php
class Copy { public $p = 1; }
$o = new Copy;
$ref =& $o->p;
unset($ref);
$clone = clone $o;
$clone->p = 2;
echo $o->p, $clone->p;
--EXPECT--
12
