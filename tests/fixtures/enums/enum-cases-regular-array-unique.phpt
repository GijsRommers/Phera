--TEST--
enums: enum cases regular array unique
--FILE--
<?php
enum E {case A;case B;case C;} var_dump(array_unique([E::A,E::C,E::B,E::A,E::B], SORT_REGULAR));
--EXPECT--
array(3) {
  [0]=>
  enum(E::A)
  [1]=>
  enum(E::C)
  [2]=>
  enum(E::B)
}
