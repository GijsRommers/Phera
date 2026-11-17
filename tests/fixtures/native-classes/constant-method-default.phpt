--TEST--
Native classes: constant-method-default
--FILE--
<?php
class Child extends PDO {
 public static function value($mode=self::FETCH_ASSOC) {
 return $mode;
 }
 }
 var_dump(Child::value());

--EXPECT--
int(2)
