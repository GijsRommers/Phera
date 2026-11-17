--TEST--
Native classes: constant-undefined
--FILE--
<?php
try {
 echo PDO::NOPE;
 }
 catch (Error $e) {
 echo $e->getMessage(),"\n";
 }

--EXPECT--
Undefined constant PDO::NOPE
