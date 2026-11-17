--TEST--
Native classes: exception-constructor-string-code
--FILE--
<?php
try {
 new PDOException("m","42P01");
 }
 catch (TypeError $e) {
 echo $e->getMessage(),"\n";
 }

--EXPECT--
Exception::__construct(): Argument #2 ($code) must be of type int, string given
