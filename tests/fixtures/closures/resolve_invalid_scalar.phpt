--TEST--
resolve invalid scalar
--FILE--
<?php
foreach([1,true,null,3.5]as $f){try{$f();}catch(Error $e){echo $e->getMessage(),"
";}}
--EXPECT--
Value of type int is not callable
Value of type bool is not callable
Value of type null is not callable
Value of type float is not callable
