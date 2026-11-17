--TEST--
from callable failure
--FILE--
<?php
try{Closure::fromCallable("missingClosureFixture");}catch(TypeError $e){echo $e->getMessage(),"
";}
--EXPECT--
Failed to create closure from callable: function "missingClosureFixture" not found or invalid function name
