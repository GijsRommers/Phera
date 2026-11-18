--TEST--
enums: try from found
--FILE--
<?php
enum E: int{case A=7;} var_dump(E::tryFrom(7)===E::A);
--EXPECT--
bool(true)
