--TEST--
enums: different case identity
--FILE--
<?php
enum E {case A;case B;} var_dump(E::A===E::B);
--EXPECT--
bool(false)
