--TEST--
String class operator requires object
--FILE--
<?php
$name = "stdClass";
try { var_dump($name::class); } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Cannot use "::class" on string
