--TEST--
Unset reports its own invalid array offset diagnostic
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
try { unset($b->data[[]]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Cannot unset offset of type array on array
