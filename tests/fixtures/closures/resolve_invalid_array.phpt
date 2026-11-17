--TEST--
resolve invalid array
--FILE--
<?php
foreach([[],[1],[1,"x"],["stdClass",1]]as $f){try{$f();}catch(Error $e){echo $e->getMessage(),"
";}}
--EXPECT--
Array callback must have exactly two elements
Array callback must have exactly two elements
First array member is not a valid class name or object
Second array member is not a valid method
