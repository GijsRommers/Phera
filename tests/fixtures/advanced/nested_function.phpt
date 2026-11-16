--TEST--
nested_function
--FILE--
<?php
function outer(){function inner(){echo 42;}}outer();inner();
--EXPECT--
42