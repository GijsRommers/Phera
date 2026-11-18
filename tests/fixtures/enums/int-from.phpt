--TEST--
enums: int from
--FILE--
<?php
enum E: int{case A=7;} var_dump(E::from(7)===E::A);
--EXPECT--
bool(true)
