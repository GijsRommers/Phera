--TEST--
enums: different case loose equality
--FILE--
<?php
enum E {case A;case B;} var_dump(E::A==E::B);
--EXPECT--
bool(false)
