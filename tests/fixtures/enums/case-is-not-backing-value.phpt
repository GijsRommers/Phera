--TEST--
enums: case is not backing value
--FILE--
<?php
enum E: int {case A=1;} var_dump(E::A==1,1<E::A);
--EXPECT--
bool(false)
bool(false)
