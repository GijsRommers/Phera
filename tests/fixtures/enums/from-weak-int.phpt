--TEST--
enums: from weak int
--FILE--
<?php
enum E: int{case A=7;} var_dump(E::from("7")===E::A);
--EXPECT--
bool(true)
