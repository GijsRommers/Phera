--TEST--
Native classes: constant-static
--FILE--
<?php
class Child extends PDO {
 public static function value() {
 return static::FETCH_ASSOC;
 }
 }
 class Grandchild extends Child {
 const FETCH_ASSOC=42;
 }
 var_dump(Child::value(), Grandchild::value());

--EXPECTF--

Fatal error: Type of Grandchild::FETCH_ASSOC must be compatible with PDO::FETCH_ASSOC of type int in %s on line 7
