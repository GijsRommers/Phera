--TEST--
Native classes: constant-undefined-function
--FILE--
<?php
try {
 constant("PDO::NOPE");
 }
 catch (Error $e) {
 echo $e->getMessage(),"\n";
 }

--EXPECT--
Undefined constant PDO::NOPE
