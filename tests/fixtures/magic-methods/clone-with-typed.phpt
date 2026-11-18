--TEST--
Clone property updates check declared types
--FILE--
<?php
class Copy { public int $value = 1; }
try { clone(new Copy, ["value" => []]); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
Cannot assign array to property Copy::$value of type int
