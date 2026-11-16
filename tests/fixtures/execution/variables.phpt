--TEST--
variables
--FILE--
<?php
$name='x';$$name=42;echo $x; $x=43;echo $$name;
--EXPECT--
4243