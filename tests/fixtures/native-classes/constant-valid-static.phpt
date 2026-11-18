--TEST--
Native classes: constant-valid-static
--FILE--
<?php
class Child extends PDO {
 public static function value() {
 return static::FETCH_ASSOC;
 }
 }
 class Grandchild extends Child {
 const int FETCH_ASSOC=42;
 }
 var_dump(Child::value(), Grandchild::value());

--EXPECT--
int(2)
int(42)
