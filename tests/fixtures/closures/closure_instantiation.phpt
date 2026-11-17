--TEST--
closure instantiation
--FILE--
<?php
try{new Closure;}catch(Error $e){echo $e->getMessage(),"
";}
--EXPECT--
Instantiation of class Closure is not allowed
