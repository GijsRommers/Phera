--TEST--
Native classes: constant-invalid-override-type
--FILE--
<?php
class Child extends PDO {
 public const string FETCH_ASSOC="x";
 }

--EXPECTF--

Fatal error: Type of Child::FETCH_ASSOC must be compatible with PDO::FETCH_ASSOC of type int in %s on line 2
