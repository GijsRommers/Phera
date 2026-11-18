--TEST--
A nullable scalar property rejects array initialization before converting its key
--FILE--
<?php
class Box { public ?int $data = null; }
$b = new Box;
try { $b->data[1.5] = 1; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump($b->data);
--EXPECT--
Cannot auto-initialize an array inside property Box::$data of type ?int
NULL
