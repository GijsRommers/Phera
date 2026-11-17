--TEST--
callback empty map invalid
--FILE--
<?php
try{array_map("missingClosureFixture",[]);}catch(TypeError $e){echo $e->getMessage(),"
";}
--EXPECT--
array_map(): Argument #1 ($callback) must be a valid callback or null, function "missingClosureFixture" not found or invalid function name
