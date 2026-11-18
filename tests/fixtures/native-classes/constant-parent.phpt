--TEST--
Native classes: constant-parent
--FILE--
<?php
class Child extends PDO {
 public static function value() {
 return parent::FETCH_NUM;
 }
 }
 var_dump(Child::value());

--EXPECT--
int(3)
