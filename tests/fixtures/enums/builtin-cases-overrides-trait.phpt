--TEST--
enums: builtin cases overrides trait
--FILE--
<?php
trait T{static function cases(){return "trait";}} enum E{use T;case A;} var_dump(E::cases());
--EXPECT--
array(1) {
  [0]=>
  enum(E::A)
}
