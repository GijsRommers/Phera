--TEST--
Clone property updates respect private visibility
--FILE--
<?php
class Copy { private $value = 1; }
try { clone(new Copy, ["value" => 3]); } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Cannot access private property Copy::$value
