--TEST--
Native classes: constant-override
--FILE--
<?php
class Child extends PDO {
 public const FETCH_ASSOC=99;
 public static function value() {
 return [self::FETCH_ASSOC, parent::FETCH_ASSOC];
 }
 }
 var_dump(Child::FETCH_ASSOC, PDO::FETCH_ASSOC, Child::value());

--EXPECTF--

Fatal error: Type of Child::FETCH_ASSOC must be compatible with PDO::FETCH_ASSOC of type int in %s on line 2
