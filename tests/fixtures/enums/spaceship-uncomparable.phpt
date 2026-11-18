--TEST--
enums: spaceship uncomparable
--FILE--
<?php
enum E {case A;case B;} var_dump(E::A<=>E::B,E::B<=>E::A);
--EXPECT--
int(1)
int(1)
