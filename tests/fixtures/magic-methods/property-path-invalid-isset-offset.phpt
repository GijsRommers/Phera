--TEST--
Isset identifies invalid array keys at the dimension that is read
--FILE--
<?php
class Box { public array $data = []; }
$b = new Box;
try { var_dump(isset($b->data[[]])); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Cannot access offset of type array in isset or empty
