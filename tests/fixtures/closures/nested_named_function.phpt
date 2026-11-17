--TEST--
nested named function
--FILE--
<?php
$f=function(){function nestedClosureHelper($x){return $x*2;}return nestedClosureHelper(7);};var_dump($f());
--EXPECT--
int(14)
