--TEST--
Clone-with respects the prohibition on Closure dynamic properties
--FILE--
<?php
try { clone(fn() => 3, ["property" => 1]); }
catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Cannot create dynamic property Closure::$property
