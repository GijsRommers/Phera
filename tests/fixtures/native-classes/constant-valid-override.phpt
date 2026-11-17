--TEST--
Native classes: constant-valid-override
--FILE--
<?php
class Child extends PDO {
 public const int FETCH_ASSOC=99;
 public static function value() {
 return [self::FETCH_ASSOC, parent::FETCH_ASSOC];
 }
 }
 var_dump(Child::FETCH_ASSOC, PDO::FETCH_ASSOC, Child::value());

--EXPECT--
int(99)
int(2)
array(2) {
  [0]=>
  int(99)
  [1]=>
  int(2)
}
