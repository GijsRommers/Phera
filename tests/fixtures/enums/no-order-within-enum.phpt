--TEST--
enums: no order within enum
--FILE--
<?php
enum E {case A;case B;} var_dump(E::A<E::B,E::B<E::A);
--EXPECT--
bool(false)
bool(false)
