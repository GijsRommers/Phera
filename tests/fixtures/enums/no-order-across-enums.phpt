--TEST--
enums: no order across enums
--FILE--
<?php
enum E {case A;} enum F {case B;} var_dump(E::A<F::B,F::B<E::A);
--EXPECT--
bool(false)
bool(false)
