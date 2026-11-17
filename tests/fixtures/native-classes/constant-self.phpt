--TEST--
Native classes: constant-self
--FILE--
<?php
class Child extends PDO {
 public static function value() {
 return self::FETCH_ASSOC;
 }
 }
 var_dump(Child::value());

--EXPECT--
int(2)
